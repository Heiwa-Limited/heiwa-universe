#![cfg(unix)]
//! Exercise the public admitted adapter, not the raw transport: the admission
//! wrapper must preserve host tools and cancellation across provider launchers.
use heiwa_provider::adapter::{
    Message, ProviderTools, Role, StreamEvent, ToolDefinition, ToolResponse,
};
use heiwa_provider::{
    AccountRegistry, AccountStatus, Credential, DetectedModel, InventoryTruth, PriceTruth,
    ProviderAccount,
};
use serde_json::json;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;
use tokio::sync::mpsc;

static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());
struct Fixture {
    _root: tempfile::TempDir,
    binary: String,
    pid: std::path::PathBuf,
    old_path: Option<std::ffi::OsString>,
    _guard: std::sync::MutexGuard<'static, ()>,
}
impl Fixture {
    fn new(mode: &str) -> Self {
        let guard = ENV.lock().unwrap_or_else(|poison| poison.into_inner());
        let old_path = std::env::var_os("PATH");
        let root = tempfile::tempdir().unwrap();
        std::env::set_var("PATH", root.path());
        let binary = root.path().join("codex");
        let pid = root.path().join("pids");
        let program = format!(
            "#!/usr/bin/python3\nMODE={}\nPIDFILE={}\n",
            json!(mode),
            json!(pid)
        );
        let body = r#"
import json,os,sys,subprocess,time
assert sys.argv[1:4] == ['app-server','--listen','stdio://']
# No prompt or secret may be copied into argv.
assert 'literal $(touch' not in ' '.join(sys.argv)
def send(value):
 print(json.dumps(value),flush=True)
def response(value,result):
 send({'id':value['id'],'result':result})
with open(PIDFILE,'w') as f:f.write(str(os.getpid()))
if MODE=='stderr':sys.stderr.write('x'*1000000);sys.stderr.flush()
for line in sys.stdin:
 value=json.loads(line);method=value.get('method');params=value.get('params',{})
 if method=='initialize':
  assert params['clientInfo']['name']=='heiwa'
  assert params['capabilities']['experimentalApi']
  response(value,{})
 elif method=='account/read':
  response(value,{'account':{'type':'apiKey' if MODE=='api-key' else 'chatgpt'}})
 elif method=='config/read':
  response(value,{'config':{'mcp_servers':{'unrelated':{'enabled':True,'command':'private-not-copied'}},'plugins':{'peer@vendor':{'enabled':True}}}})
 elif method=='thread/start':
  assert params['sandbox']=='read-only' and params['approvalPolicy']=='never'
  assert params['config']['mcp_servers']['unrelated']['enabled'] is False
  assert params['config']['plugins']['peer@vendor']['enabled'] is False
  assert params['config']['features']['shell_tool'] is False
  if MODE=='tools':assert params['dynamicTools'][0]['name']=='fs__read'
  response(value,{'thread':{'id':'fixture-thread'},'modelProvider':'openai'})
 elif method=='thread/inject_items':
  assert [item['role'] for item in params['items']]==['user','assistant']
  response(value,{})
 elif method=='turn/start':
  assert params['input'][0]['text']=='literal $(touch should-not-run) `not-code`'
  if MODE=='eof':sys.exit(0)
  if MODE=='invalid':os.write(1,b'not-json\n');continue
  if MODE=='utf8':os.write(1,b'\xff\n');continue
  if MODE=='failed':
   send({'method':'turn/completed','params':{'turn':{'status':'failed','error':{'message':'fixture denied'}}}});continue
  if MODE=='native':
   send({'method':'item/started','params':{'item':{'type':'commandExecution'}}});continue
  if MODE=='cancel':
   child=subprocess.Popen(['/bin/sleep','30'])
   with open(PIDFILE,'a') as f:f.write('\n'+str(child.pid))
   continue
  if MODE=='tools':
   send({'id':'host-1','method':'item/tool/call','params':{'threadId':'fixture-thread','turnId':'turn','callId':'call-one','tool':'fs__read','arguments':{'path':'proof.txt'}}});continue
  send({'method':'item/agentMessage/delta','params':{'itemId':'answer','delta':'verified answer'}})
  send({'method':'item/completed','params':{'item':{'type':'agentMessage','id':'answer','text':'verified answer'}}})
  send({'method':'thread/tokenUsage/updated','params':{'tokenUsage':{'total':{'inputTokens':11,'outputTokens':7,'cachedInputTokens':3}}}})
  send({'method':'turn/completed','params':{'turn':{'status':'completed'}}})
 elif value.get('id')=='host-1':
  assert value['result']['success'] and value['result']['contentItems'][0]['text']=='fixture host content'
  # A retransmitted call ID must reuse the result, never repeat the effect.
  send({'id':'host-2','method':'item/tool/call','params':{'threadId':'fixture-thread','turnId':'turn','callId':'call-one','tool':'fs__read','arguments':{'path':'proof.txt'}}})
 elif value.get('id')=='host-2':
  send({'method':'item/agentMessage/delta','params':{'itemId':'answer','delta':'tool verified'}})
  send({'method':'turn/completed','params':{'turn':{'status':'completed'}}})
"#;
        std::fs::write(&binary, program + body).unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        Self {
            _root: root,
            binary: "codex".into(),
            old_path,
            _guard: guard,
            pid,
        }
    }
    fn adapter(&self) -> std::sync::Arc<dyn heiwa_provider::adapter::ProviderAdapter> {
        let model = DetectedModel {
            model_id: "fixture".into(),
            provider_model_id: "fixture".into(),
            provider: "openai".into(),
            account_id: "fixture-account".into(),
            rate_group: "openai_sub".into(),
            capability_class: 3,
            context_window: 32000,
            supports_streaming: true,
            supports_tools: true,
            supports_vision: false,
            supports_audio: false,
            cost_per_1k_input: 0.0,
            cost_per_1k_output: 0.0,
            price_truth: PriceTruth::Unknown,
            inventory_truth: InventoryTruth::Verified,
        };
        let registry = AccountRegistry::from_accounts(vec![ProviderAccount {
            account_id: model.account_id.clone(),
            provider: "openai".into(),
            credential: Credential::OauthCli {
                binary: self.binary.clone(),
            },
            rate_group: "openai_sub".into(),
            status: AccountStatus::Connected,
            models: vec![model],
        }]);
        heiwa_provider::routing::resolve_adapter_with(&registry, "codex", "fixture", None).unwrap()
    }
    async fn assert_reaped(&self) {
        if let Ok(pids) = std::fs::read_to_string(&self.pid) {
            for _ in 0..100 {
                let alive = pids.lines().any(|pid| {
                    let output = std::process::Command::new("/bin/ps")
                        .args(["-p", pid, "-o", "stat="])
                        .output()
                        .unwrap();
                    output.status.success()
                        && !String::from_utf8_lossy(&output.stdout)
                            .trim()
                            .starts_with('Z')
                });
                if !alive {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            panic!("provider launcher or descendant outlived the adapter");
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(value) = &self.old_path {
            std::env::set_var("PATH", value);
        } else {
            std::env::remove_var("PATH");
        }
    }
}
fn messages() -> Vec<Message> {
    vec![
        Message {
            role: Role::System,
            content: "Heiwa test instructions".into(),
        },
        Message {
            role: Role::User,
            content: "prior user".into(),
        },
        Message {
            role: Role::Assistant,
            content: "prior assistant".into(),
        },
        Message {
            role: Role::User,
            content: "literal $(touch should-not-run) `not-code`".into(),
        },
    ]
}
async fn run(mode: &str) -> Vec<StreamEvent> {
    let fixture = Fixture::new(mode);
    let adapter = fixture.adapter();
    let (tx, mut rx) = mpsc::channel(32);
    tokio::time::timeout(
        Duration::from_secs(5),
        adapter.send("fixture", &messages(), tx),
    )
    .await
    .unwrap()
    .unwrap();
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    fixture.assert_reaped().await;
    events
}
#[tokio::test]
async fn app_server_preserves_roles_text_usage_and_literal_input() {
    let events = run("success").await;
    assert_eq!(
        events
            .iter()
            .filter_map(|e| if let StreamEvent::Token(t) = e {
                Some(t.as_str())
            } else {
                None
            })
            .collect::<String>(),
        "verified answer"
    );
    assert!(
        matches!(events.last(),Some(StreamEvent::Done(u)) if u.input_tokens==11 && u.output_tokens==7 && u.cache_read_tokens==3)
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, StreamEvent::Done(_) | StreamEvent::Error(_)))
            .count(),
        1
    );
}
#[tokio::test]
async fn admission_wrapper_preserves_native_tools_and_retransmits_without_repeating_effects() {
    let fixture = Fixture::new("tools");
    let adapter = fixture.adapter();
    let (tx, mut rx) = mpsc::channel(32);
    let (requests, mut calls) = mpsc::channel::<heiwa_provider::adapter::ProviderToolRequest>(4);
    let tools = ProviderTools {
        definitions: vec![ToolDefinition {
            name: "fs.read".into(),
            description: "read fixture".into(),
            input_schema: json!({"type":"object","properties":{"path":{"type":"string"}}}),
        }],
        requests: Some(requests),
        ..Default::default()
    };
    let dispatcher = tokio::spawn(async move {
        let mut count = 0;
        while let Some(call) = calls.recv().await {
            assert_eq!(call.name, "fs.read");
            count += 1;
            call.response
                .send(ToolResponse {
                    output: "fixture host content".into(),
                    success: true,
                })
                .unwrap();
        }
        count
    });
    tokio::time::timeout(
        Duration::from_secs(5),
        adapter.send_with_tools("fixture", &messages(), tx, tools),
    )
    .await
    .unwrap()
    .unwrap();
    let mut text = String::new();
    let mut done = false;
    while let Some(event) = rx.recv().await {
        match event {
            StreamEvent::Token(t) => text.push_str(&t),
            StreamEvent::Done(_) => done = true,
            StreamEvent::Error(e) => panic!("{e}"),
            _ => {}
        }
    }
    assert!(done);
    assert_eq!(text, "tool verified");
    assert_eq!(dispatcher.await.unwrap(), 1);
    fixture.assert_reaped().await;
}
#[tokio::test]
async fn malformed_failed_incomplete_and_native_action_streams_are_never_success() {
    for mode in ["failed", "eof", "invalid", "utf8", "native", "api-key"] {
        let events = run(mode).await;
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, StreamEvent::Error(_)))
                .count(),
            1,
            "{mode}: {events:?}"
        );
        assert!(
            !events.iter().any(|e| matches!(e, StreamEvent::Done(_))),
            "{mode}"
        );
    }
}
#[tokio::test]
async fn stderr_cannot_block_the_protocol() {
    assert!(matches!(
        run("stderr").await.last(),
        Some(StreamEvent::Done(_))
    ));
}
#[tokio::test]
async fn closed_consumer_stops_the_launcher_and_its_worker() {
    let fixture = Fixture::new("cancel");
    let adapter = fixture.adapter();
    let (tx, rx) = mpsc::channel(8);
    let message = messages();
    let task = tokio::spawn(async move { adapter.send("fixture", &message, tx).await });
    for _ in 0..100 {
        if std::fs::read_to_string(&fixture.pid)
            .unwrap_or_default()
            .lines()
            .count()
            == 2
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        std::fs::read_to_string(&fixture.pid)
            .unwrap()
            .lines()
            .count(),
        2
    );
    drop(rx);
    tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    fixture.assert_reaped().await;
}
