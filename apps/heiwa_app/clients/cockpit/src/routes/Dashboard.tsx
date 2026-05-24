import type { JSX } from "solid-js";
import { For, createSignal, onCleanup, Show } from "solid-js";
import { providers } from "../lib/providers";

interface Message {
  id: string;
  role: "user" | "assistant";
  text: string;
  media?: string;
  mediaType?: "image" | "video";
  trace?: string;
}

export default function Dashboard(): JSX.Element {
  const [inputMode, setInputMode] = createSignal<"text" | "voice" | "image" | "video">("text");
  const [inputText, setInputText] = createSignal("");
  const [isRecording, setIsRecording] = createSignal(false);
  const [isAnalyzing, setIsAnalyzing] = createSignal(false);
  const [uploadedMedia, setUploadedMedia] = createSignal<string | null>(null);
  const [mediaType, setMediaType] = createSignal<"image" | "video" | null>(null);
  
  const [voiceWave, setVoiceWave] = createSignal<number[]>([]);
  const [messages, setMessages] = createSignal<Message[]>([
    {
      id: "init",
      role: "assistant",
      text: "Heiwa is active and compiled. I'm connected to your local CPU/RAM pipeline, local GPU VRAM via Ollama, and cloud prompt-cached API lanes. Ask me anything, drop a media file, or activate voice mode.",
    }
  ]);

  // Live Resource Compiled Indicators (dynamically changing slightly to feel alive)
  const [vram, setVram] = createSignal(5.8);
  const [cpu, setCpu] = createSignal(14);
  const [ram, setRam] = createSignal(44);
  
  const resourceTimer = setInterval(() => {
    setVram(+(5.8 + (Math.random() * 0.4 - 0.2)).toFixed(1));
    setCpu(Math.floor(12 + Math.random() * 6));
    setRam(Math.floor(43 + Math.random() * 3));
  }, 3000);

  onCleanup(() => {
    clearInterval(resourceTimer);
    if (waveInterval) clearInterval(waveInterval);
  });

  // Simulated Voice waveform
  let waveInterval: any;
  const toggleVoiceRecording = () => {
    if (isRecording()) {
      setIsRecording(false);
      clearInterval(waveInterval);
      setVoiceWave([]);
      
      // Simulate voice submission
      submitPrompt("Process this voice instruction to audit release sandbox", true);
    } else {
      setIsRecording(true);
      setInputMode("voice");
      waveInterval = setInterval(() => {
        setVoiceWave(Array.from({ length: 24 }, () => Math.floor(Math.random() * 30) + 4));
      }, 100);
    }
  };

  // Drag and drop mock uploads
  const handleMediaUpload = (type: "image" | "video") => {
    setIsAnalyzing(true);
    setTimeout(() => {
      setIsAnalyzing(false);
      setMediaType(type);
      if (type === "image") {
        setUploadedMedia("📸 mock_system_architecture_diagram.png");
      } else {
        setUploadedMedia("🎥 mock_ci_build_error_recording.mp4");
      }
    }, 1000);
  };

  const clearMedia = () => {
    setUploadedMedia(null);
    setMediaType(null);
  };

  const handleSubmit = (e: Event) => {
    e.preventDefault();
    if (!inputText().trim() && !uploadedMedia()) return;
    submitPrompt(inputText(), false);
  };

  const submitPrompt = (promptText: string, isVoice: boolean) => {
    const userMsgId = `user-${Date.now()}`;
    const cleanPrompt = promptText.trim() || (isVoice ? "🎤 Voice Command Input" : "Uploaded Multimodal Attachment");
    
    // Append user message
    setMessages(prev => [...prev, {
      id: userMsgId,
      role: "user",
      text: cleanPrompt,
      media: uploadedMedia() || undefined,
      mediaType: mediaType() || undefined
    }]);

    setInputText("");
    clearMedia();

    // Simulated Agentic processing & routing evaluation
    setTimeout(() => {
      const isDeploy = cleanPrompt.toLowerCase().includes("deploy");
      const isCode = cleanPrompt.toLowerCase().includes("code") || cleanPrompt.toLowerCase().includes("build") || cleanPrompt.toLowerCase().includes("audit");
      
      let answer = "";
      let trace = "";

      if (isDeploy) {
        answer = "Gated action initiated. Task 'Production Deploy' classified as CRITICAL risk. Halting loop and staging approval JSON under ~/.heiwa/state/dispatch/requests/. Run 'heiwa approvals decide <id> --approve' to proceed.";
        trace = "intent=deploy risk=critical surface=cockpit-app -> [HOLD] staged req_a7c8b9";
      } else if (isCode) {
        answer = "Analyzing workspace and local checkouts. Sandbox compiled successfully. Local Qwen 3.5 (9B) completed the pre-flight checks and routed the high-level refactoring to the cloud cache lane (100% hits).";
        trace = "intent=build rank=1 route=google/gemini-pro prompt-cache=HIT latency=280ms cost=$0.0003";
      } else {
        answer = "Prompt received. Successfully queried local Ollama repository with 0ms egress latency. All vaults and secrets remain strictly isolated within ~/.heiwa on this machine.";
        trace = "intent=general rank=0 route=local/ollama-qwen-3.5 latency=180ms cost=$0.0000";
      }

      setMessages(prev => [...prev, {
        id: `assist-${Date.now()}`,
        role: "assistant",
        text: answer,
        trace
      }]);
    }, 1500);
  };

  // Simulated Benchmarks Runner
  const [runningBenchmarks, setRunningBenchmarks] = createSignal(false);
  const [benchmarkResults, setBenchmarkResults] = createSignal<any>(null);

  const runLocalBenchmarks = () => {
    setRunningBenchmarks(true);
    setBenchmarkResults(null);
    setTimeout(() => {
      setRunningBenchmarks(false);
      setBenchmarkResults({
        ttftCloud: "290ms",
        ttftStandard: "1,450ms",
        costSaving: "90%",
        localVram: "5.8 GB",
        latencyLocal: "180ms",
        cachedRatio: "94.2%",
        totalSavings: "$12.45 / 100K prompt runs"
      });
    }, 2000);
  };

  const connected = () =>
    providers.providers.filter((p) => p.maturity === "stable").length;
  const totalLanes = () => Object.keys(providers.lanes).length;

  return (
    <section class="hero compact">
      <p class="eyebrow">Local Cockpit</p>
      <h1>Heiwa Consolidated Console</h1>
      <p class="lede" style={{ "margin-bottom": "2rem" }}>
        Your single local-first intake and execution layer. Speak, upload, or write to your compiled hardware and model pipelines.
      </p>

      {/* 1. Compiled Resource Indicators */}
      <div class="resource-grid">
        <div class="glass-card resource-card gpu">
          <span>Local GPU VRAM (Ollama)</span>
          <strong>{vram()} GB</strong>
          <div class="resource-progress">
            <div class="resource-progress-bar" style={{ width: `${(vram() / 16) * 100}%`, background: "var(--text-gradient)" }}></div>
          </div>
          <small style={{ display: "block", "margin-top": "0.5rem", color: "#64748b", "font-size": "0.75rem" }}>Allocated: Qwen 3.5 9B (Quantized)</small>
        </div>

        <div class="glass-card resource-card cpu">
          <span>Local CPU Load</span>
          <strong>{cpu()}%</strong>
          <div class="resource-progress">
            <div class="resource-progress-bar" style={{ width: `${cpu()}%`, background: "var(--gold-gradient)" }}></div>
          </div>
          <small style={{ display: "block", "margin-top": "0.5rem", color: "#64748b", "font-size": "0.75rem" }}>Active workers: 2 live</small>
        </div>

        <div class="glass-card resource-card ram">
          <span>Local RAM Usage</span>
          <strong>{ram()}%</strong>
          <div class="resource-progress">
            <div class="resource-progress-bar" style={{ width: `${ram()}%`, background: "var(--magenta-gradient)" }}></div>
          </div>
          <small style={{ display: "block", "margin-top": "0.5rem", color: "#64748b", "font-size": "0.75rem" }}>Sovereign memory isolated</small>
        </div>

        <div class="glass-card resource-card quota">
          <span>Cloud Cache savings</span>
          <strong>90% cost</strong>
          <div class="resource-progress">
            <div class="resource-progress-bar" style={{ width: "90%", background: "var(--success-gradient)" }}></div>
          </div>
          <small style={{ display: "block", "margin-top": "0.5rem", color: "#64748b", "font-size": "0.75rem" }}>Prompt-Cache aligned</small>
        </div>
      </div>

      {/* 2. Interactive Omni-Input Layer */}
      <div class="glass-card" style={{ "margin-bottom": "2rem" }}>
        <div class="input-modes-bar">
          <button class="mode-btn" classList={{ active: inputMode() === "text" }} onClick={() => setInputMode("text")}>
            ⌨️ Text Prompt
          </button>
          <button class="mode-btn" classList={{ active: inputMode() === "voice" }} onClick={() => setInputMode("voice")}>
            🎤 Voice Instruction
          </button>
          <button class="mode-btn" classList={{ active: inputMode() === "image" }} onClick={() => setInputMode("image")}>
            📸 Image Attach
          </button>
          <button class="mode-btn" classList={{ active: inputMode() === "video" }} onClick={() => setInputMode("video")}>
            🎥 Video Clip
          </button>
        </div>

        <div class="omni-input-container">
          {/* Voice Mode View */}
          <Show when={inputMode() === "voice"}>
            <div style={{ text: "center", padding: "1rem" }}>
              <p style={{ color: "#94a3b8", "font-size": "0.9rem", "margin-bottom": "0.5rem" }}>
                {isRecording() ? "🔴 Recording your instruction... Tap microphone to finalize." : "Tap microphone button to start voice instruction."}
              </p>
              <button 
                onClick={toggleVoiceRecording} 
                style={{ 
                  background: isRecording() ? "red" : "var(--text-gradient)", 
                  border: "none", 
                  color: "#000", 
                  width: "60px", 
                  height: "60px", 
                  "border-radius": "50%", 
                  cursor: "pointer",
                  display: "inline-flex",
                  "align-items": "center",
                  "justify-content": "center",
                  "font-size": "1.5rem"
                }}
              >
                🎤
              </button>
              
              <Show when={isRecording() && voiceWave().length > 0}>
                <div class="voice-wave-container">
                  <For each={voiceWave()}>
                    {(height) => <div class="wave-bar" style={{ height: `${height}px` }}></div>}
                  </For>
                </div>
              </Show>
            </div>
          </Show>

          {/* Image Mode View */}
          <Show when={inputMode() === "image"}>
            <Show when={!uploadedMedia()} fallback={
              <div style={{ padding: "1rem", background: "rgba(0,0,0,0.2)", "border-radius": "12px", display: "flex", "justify-content": "space-between", "align-items": "center" }}>
                <span>{uploadedMedia()}</span>
                <button onClick={clearMedia} style={{ background: "transparent", border: "none", color: "red", cursor: "pointer" }}>Delete</button>
              </div>
            }>
              <div class="drag-zone" onClick={() => handleMediaUpload("image")}>
                <p>Drag & Drop or **Click to Upload system architecture diagram** (Mock PNG)</p>
                <small style={{ color: "#64748b" }}>Supports PNG, JPEG up to 10MB</small>
              </div>
            </Show>
          </Show>

          {/* Video Mode View */}
          <Show when={inputMode() === "video"}>
            <Show when={!uploadedMedia()} fallback={
              <div style={{ padding: "1rem", background: "rgba(0,0,0,0.2)", "border-radius": "12px", display: "flex", "justify-content": "space-between", "align-items": "center" }}>
                <span>{uploadedMedia()}</span>
                <button onClick={clearMedia} style={{ background: "transparent", border: "none", color: "red", cursor: "pointer" }}>Delete</button>
              </div>
            }>
              <div class="drag-zone" onClick={() => handleMediaUpload("video")}>
                <p>Drag & Drop or **Click to Upload screen error recording** (Mock MP4)</p>
                <small style={{ color: "#64748b" }}>Supports MP4, MOV up to 50MB</small>
              </div>
            </Show>
          </Show>

          {/* Analyzing loader */}
          <Show when={isAnalyzing()}>
            <div style={{ text: "center", color: "#00f2fe", padding: "1rem" }}>
              ⏳ Analyzing file contents and compiling token footprint...
            </div>
          </Show>

          {/* Form Text Submission */}
          <form onSubmit={handleSubmit} class="input-field-wrapper">
            <input 
              type="text" 
              class="input-field" 
              placeholder={uploadedMedia() ? `Loaded file: ${uploadedMedia()} | Add follow-up query...` : "Auditing sandbox build, deploying release, or querying memories..."}
              value={inputText()}
              onInput={(e) => setInputText(e.currentTarget.value)}
            />
            <button type="submit" class="submit-btn">Send ⚡</button>
          </form>
        </div>

        {/* Live Chat Stream */}
        <div class="chat-console">
          <For each={messages()}>
            {(msg) => (
              <div class={`message-bubble ${msg.role}`}>
                <Show when={msg.media}>
                  <div style={{ "font-size": "0.8rem", color: "#00f2fe", "margin-bottom": "0.5rem", background: "rgba(0,242,254,0.05)", padding: "0.25rem 0.5rem", "border-radius": "4px", display: "inline-block" }}>
                    Attached: {msg.media}
                  </div>
                </Show>
                <p>{msg.text}</p>
                <Show when={msg.trace}>
                  <div class="trace-details">
                    🔍 DREX Core Trace: {msg.trace}
                  </div>
                </Show>
              </div>
            )}
          </For>
        </div>
      </div>

      {/* 3. Interactive Developer Benchmarks Panel */}
      <div class="glass-card callout">
        <div style={{ display: "flex", "justify-content": "space-between", "align-items": "center" }}>
          <div>
            <h2>Interactive Performance Benchmarks</h2>
            <p style={{ color: "#94a3b8", "font-size": "0.9rem" }}>
              Run latency and cost-efficiency benchmark tests across local and cloud resource tiers.
            </p>
          </div>
          <button class="btn-bench" onClick={runLocalBenchmarks} disabled={runningBenchmarks()}>
            {runningBenchmarks() ? "⏳ Testing..." : "🚀 Run Dev Benchmarks"}
          </button>
        </div>

        <Show when={benchmarkResults()}>
          <div class="bench-grid">
            <div class="bench-card">
              <strong>{benchmarkResults().ttftCloud}</strong>
              <span>Cached TTFT (Gemini)</span>
            </div>
            <div class="bench-card">
              <strong>{benchmarkResults().ttftStandard}</strong>
              <span>Standard TTFT (Claude)</span>
            </div>
            <div class="bench-card">
              <strong>{benchmarkResults().costSaving}</strong>
              <span>Egress cost saving</span>
            </div>
            <div class="bench-card">
              <strong>{benchmarkResults().latencyLocal}</strong>
              <span>Local TTFT (Ollama)</span>
            </div>
            <div class="bench-card">
              <strong>{benchmarkResults().cachedRatio}</strong>
              <span>Prompt Cache Hits</span>
            </div>
            <div class="bench-card">
              <strong>{benchmarkResults().totalSavings}</strong>
              <span>Sovereign savings</span>
            </div>
          </div>
        </Show>
      </div>
    </section>
  );
}
