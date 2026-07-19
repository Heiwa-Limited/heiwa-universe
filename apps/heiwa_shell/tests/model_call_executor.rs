mod model_call {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;

    use anyhow::Result;
    use async_trait::async_trait;
    use heiwa_core::drex::{
        CallRisk, CostTruth, ExecutionLocality, ModelCallCandidate, ModelCallRequest,
        ModelCallStage, PrivacyClass, SafetyClass,
    };
    use heiwa_evidence::{OperatorEventType, OperatorJournal};
    use heiwa_protocol::ModelTier;
    use heiwa_provider::adapter::{Message, ProviderAdapter, Role, StreamEvent, TokenUsage};
    use heiwa_session::operator::{OperatorSessionService, StartTurnRequest};
    use heiwa_shell::model_calls::{
        ModelCallError, ModelCallExecution, ModelCallExecutor, ProviderFailureClass,
    };
    use tokio::sync::{mpsc, watch};

    struct EventAdapter {
        events: Vec<StreamEvent>,
    }

    struct CountingErrorAdapter {
        sends: Arc<AtomicUsize>,
        error: String,
        sabotage_stream: Option<PathBuf>,
    }

    #[async_trait]
    impl ProviderAdapter for CountingErrorAdapter {
        async fn send(
            &self,
            _model: &str,
            _messages: &[Message],
            stream_tx: mpsc::Sender<StreamEvent>,
        ) -> Result<()> {
            self.sends.fetch_add(1, Ordering::SeqCst);
            if let Some(path) = &self.sabotage_stream {
                let backup = path.with_extension("jsonl.backup");
                std::fs::rename(path, backup)?;
                std::fs::create_dir(path)?;
            }
            stream_tx
                .send(StreamEvent::Error(self.error.clone()))
                .await?;
            Ok(())
        }

        async fn interrupt(&self) -> Result<()> {
            Ok(())
        }

        fn supported_models(&self) -> Vec<String> {
            vec![]
        }
    }

    struct BlockingAdapter {
        started: Arc<tokio::sync::Notify>,
        interrupted: Arc<AtomicBool>,
    }

    #[async_trait]
    impl ProviderAdapter for BlockingAdapter {
        async fn send(
            &self,
            _model: &str,
            _messages: &[Message],
            _stream_tx: mpsc::Sender<StreamEvent>,
        ) -> Result<()> {
            self.started.notify_one();
            std::future::pending::<()>().await;
            Ok(())
        }

        async fn interrupt(&self) -> Result<()> {
            self.interrupted.store(true, Ordering::SeqCst);
            Ok(())
        }

        fn supported_models(&self) -> Vec<String> {
            vec![]
        }
    }

    #[async_trait]
    impl ProviderAdapter for EventAdapter {
        async fn send(
            &self,
            _model: &str,
            _messages: &[Message],
            stream_tx: mpsc::Sender<StreamEvent>,
        ) -> Result<()> {
            for event in &self.events {
                stream_tx.send(event.clone()).await?;
            }
            Ok(())
        }

        async fn interrupt(&self) -> Result<()> {
            Ok(())
        }

        fn supported_models(&self) -> Vec<String> {
            vec![]
        }
    }

    fn candidate(id: u64, provider: &str, model: &str, cost: f64) -> ModelCallCandidate {
        ModelCallCandidate {
            tier: ModelTier {
                id,
                model_id: model.to_string(),
                provider_model_id: model.to_string(),
                provider: provider.to_string(),
                rate_group: provider.to_string(),
                capability_class: 3,
                effort_knob: "default".to_string(),
                effort_level: 1,
                cost_per_turn: cost,
                max_context_tokens: 8_192,
                strengths_json: "[\"advanced_coding\"]".to_string(),
                vram_requirement_mb: 0,
                quantization_type: "none".to_string(),
                kv_cache_strategy: "none".to_string(),
                enabled: true,
                last_success_rate: 1.0,
                avg_latency_ms: 1,
                latency_p_95_ms: 1,
                updated_at: "".to_string(),
            },
            locality: ExecutionLocality::Remote,
            connected: true,
            adapter_capable: true,
            quota_available: true,
            marginal_cost_usd: Some(cost),
            cost_truth: CostTruth::TargetOnly,
        }
    }

    fn request(thread_id: &str, turn_id: &str) -> ModelCallRequest {
        ModelCallRequest {
            thread_id: thread_id.to_string(),
            turn_id: turn_id.to_string(),
            call_id: "call-1".to_string(),
            intent: "code".to_string(),
            stage: ModelCallStage::Execution,
            raw_text: "do the work".to_string(),
            privacy: PrivacyClass::Standard,
            risk: CallRisk::Low,
            safety: SafetyClass::Approved,
            required_capabilities: vec![],
            required_context_tokens: 1,
            minimum_quality_class: 1,
            minimum_success_rate: 0.0,
            maximum_marginal_cost_usd: Some(1.0),
            preferred_provider: None,
            preferred_model: None,
            allowed_models: vec![],
            excluded_models: vec![],
        }
    }

    fn service_and_turn() -> (
        tempfile::TempDir,
        Arc<OperatorSessionService>,
        heiwa_session::operator::TurnSubmission,
    ) {
        let evidence = tempfile::tempdir().unwrap();
        let service = Arc::new(OperatorSessionService::new(
            OperatorJournal::new(evidence.path().to_path_buf()).unwrap(),
        ));
        let submission = service
            .start_turn(
                "thread-1",
                StartTurnRequest::auto("request-1", "do the work"),
            )
            .unwrap();
        (evidence, service, submission)
    }

    fn execution(
        request: ModelCallRequest,
        candidates: Vec<ModelCallCandidate>,
        max_attempts: usize,
        cancel: watch::Receiver<bool>,
    ) -> ModelCallExecution {
        ModelCallExecution {
            request,
            candidates,
            messages: vec![Message {
                role: Role::User,
                content: "do the work".to_string(),
            }],
            remaining_budget_usd: Some(1.0),
            max_attempts,
            cancel,
        }
    }

    #[tokio::test]
    async fn failed_primary_is_evidenced_before_secondary_completion() {
        let evidence = tempfile::tempdir().unwrap();
        let service = Arc::new(OperatorSessionService::new(
            OperatorJournal::new(evidence.path().to_path_buf()).unwrap(),
        ));
        let submission = service
            .start_turn(
                "thread-1",
                StartTurnRequest::auto("request-1", "do the work"),
            )
            .unwrap();

        let adapters: HashMap<String, Arc<dyn ProviderAdapter>> = HashMap::from([
            (
                "primary".to_string(),
                Arc::new(EventAdapter {
                    events: vec![StreamEvent::Error("rate_limited".to_string())],
                }) as Arc<dyn ProviderAdapter>,
            ),
            (
                "secondary".to_string(),
                Arc::new(EventAdapter {
                    events: vec![
                        StreamEvent::Token("done".to_string()),
                        StreamEvent::Done(TokenUsage {
                            input_tokens: 5,
                            output_tokens: 1,
                            cost_usd: 0.02,
                            ..TokenUsage::default()
                        }),
                    ],
                }) as Arc<dyn ProviderAdapter>,
            ),
        ]);
        let resolver =
            Arc::new(move |provider: &str, _model: &str| adapters.get(provider).cloned());
        let executor = ModelCallExecutor::new(resolver, service.clone());
        let (_cancel_tx, cancel_rx) = watch::channel(false);

        let result = executor
            .execute(ModelCallExecution {
                request: request("thread-1", &submission.turn_id),
                candidates: vec![
                    candidate(1, "primary", "primary-model", 0.01),
                    candidate(2, "secondary", "secondary-model", 0.02),
                ],
                messages: vec![Message {
                    role: Role::User,
                    content: "do the work".to_string(),
                }],
                remaining_budget_usd: Some(1.0),
                max_attempts: 3,
                cancel: cancel_rx,
            })
            .await
            .unwrap();

        assert_eq!(result.provider, "secondary");
        assert_eq!(result.model_id, "secondary-model");
        assert_eq!(result.text, "done");
        assert_eq!(result.attempts, 2);
        assert_eq!(result.failed_models, vec!["primary-model"]);

        let events = service
            .events_after("thread-1", Some(&submission.cursor), 20)
            .unwrap()
            .events;
        let kinds = events
            .iter()
            .map(|row| row.event.event_type.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            vec![
                OperatorEventType::RoutePlanned,
                OperatorEventType::RouteAttempted,
                OperatorEventType::RouteFailed,
                OperatorEventType::RoutePlanned,
                OperatorEventType::RouteAttempted,
                OperatorEventType::RouteCompleted,
            ]
        );
        assert_eq!(events[2].event.payload["failure_class"], "rate_limited");
        assert_eq!(events[3].event.payload["provider"], "secondary");
    }

    #[tokio::test]
    async fn evidence_failure_before_attempt_prevents_provider_send() {
        let (_evidence, service, submission) = service_and_turn();
        let sends = Arc::new(AtomicUsize::new(0));
        let adapter = Arc::new(CountingErrorAdapter {
            sends: sends.clone(),
            error: "unavailable".to_string(),
            sabotage_stream: None,
        }) as Arc<dyn ProviderAdapter>;
        let executor =
            ModelCallExecutor::new(Arc::new(move |_, _| Some(adapter.clone())), service.clone());
        let (_cancel_tx, cancel_rx) = watch::channel(false);
        let mut invalid = request("thread-1", &submission.turn_id);
        invalid.turn_id = "missing-turn".to_string();

        let error = executor
            .execute(execution(
                invalid,
                vec![candidate(1, "primary", "model", 0.01)],
                1,
                cancel_rx,
            ))
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            ModelCallError::EvidenceAppend {
                phase: "route_planned",
                ..
            }
        ));
        assert_eq!(sends.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn evidence_failure_after_provider_failure_prevents_fallback() {
        let (evidence, service, submission) = service_and_turn();
        let primary_sends = Arc::new(AtomicUsize::new(0));
        let secondary_sends = Arc::new(AtomicUsize::new(0));
        let stream_path = evidence.path().join("operator_events.jsonl");
        let primary = Arc::new(CountingErrorAdapter {
            sends: primary_sends.clone(),
            error: "rate_limited".to_string(),
            sabotage_stream: Some(stream_path),
        }) as Arc<dyn ProviderAdapter>;
        let secondary = Arc::new(CountingErrorAdapter {
            sends: secondary_sends.clone(),
            error: "unavailable".to_string(),
            sabotage_stream: None,
        }) as Arc<dyn ProviderAdapter>;
        let executor = ModelCallExecutor::new(
            Arc::new(move |provider, _| match provider {
                "primary" => Some(primary.clone()),
                "secondary" => Some(secondary.clone()),
                _ => None,
            }),
            service,
        );
        let (_cancel_tx, cancel_rx) = watch::channel(false);

        let error = executor
            .execute(execution(
                request("thread-1", &submission.turn_id),
                vec![
                    candidate(1, "primary", "primary-model", 0.01),
                    candidate(2, "secondary", "secondary-model", 0.02),
                ],
                3,
                cancel_rx,
            ))
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            ModelCallError::EvidenceAppend {
                phase: "route_failed",
                ..
            }
        ));
        assert_eq!(primary_sends.load(Ordering::SeqCst), 1);
        assert_eq!(secondary_sends.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn selected_none_returns_structured_no_route_error() {
        let (_evidence, service, submission) = service_and_turn();
        let executor = ModelCallExecutor::new(Arc::new(|_, _| None), service.clone());
        let (_cancel_tx, cancel_rx) = watch::channel(false);

        let error = executor
            .execute(execution(
                request("thread-1", &submission.turn_id),
                vec![],
                3,
                cancel_rx,
            ))
            .await
            .unwrap_err();

        let ModelCallError::NoRoute(plan) = error else {
            panic!("expected NoRoute");
        };
        assert!(plan.selected.is_none());
        let events = service
            .events_after("thread-1", Some(&submission.cursor), 10)
            .unwrap()
            .events;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event.event_type, OperatorEventType::RoutePlanned);
    }

    #[tokio::test]
    async fn max_attempts_zero_sends_nothing_and_values_above_three_are_capped() {
        let (_evidence, service, submission) = service_and_turn();
        let sends = Arc::new(AtomicUsize::new(0));
        let adapter = Arc::new(CountingErrorAdapter {
            sends: sends.clone(),
            error: "provider failed".to_string(),
            sabotage_stream: None,
        }) as Arc<dyn ProviderAdapter>;
        let executor =
            ModelCallExecutor::new(Arc::new(move |_, _| Some(adapter.clone())), service.clone());
        let (_cancel_tx, cancel_rx) = watch::channel(false);
        let zero = executor
            .execute(execution(
                request("thread-1", &submission.turn_id),
                vec![candidate(1, "p1", "m1", 0.01)],
                0,
                cancel_rx,
            ))
            .await
            .unwrap_err();
        assert!(matches!(zero, ModelCallError::MaxAttemptsZero));
        assert_eq!(sends.load(Ordering::SeqCst), 0);

        let (_cancel_tx, cancel_rx) = watch::channel(false);
        let error = executor
            .execute(execution(
                request("thread-1", &submission.turn_id),
                vec![
                    candidate(1, "p1", "m1", 0.01),
                    candidate(2, "p2", "m2", 0.02),
                    candidate(3, "p3", "m3", 0.03),
                    candidate(4, "p4", "m4", 0.04),
                ],
                99,
                cancel_rx,
            ))
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            ModelCallError::AttemptsExhausted { attempts: 3, .. }
        ));
        assert_eq!(sends.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn cancellation_before_and_during_stream_never_appends_completion() {
        let (_evidence, service, submission) = service_and_turn();
        let started = Arc::new(tokio::sync::Notify::new());
        let interrupted = Arc::new(AtomicBool::new(false));
        let adapter = Arc::new(BlockingAdapter {
            started: started.clone(),
            interrupted: interrupted.clone(),
        }) as Arc<dyn ProviderAdapter>;
        let executor = Arc::new(ModelCallExecutor::new(
            Arc::new(move |_, _| Some(adapter.clone())),
            service.clone(),
        ));

        let (_cancel_tx, cancel_rx) = watch::channel(true);
        let error = executor
            .execute(execution(
                request("thread-1", &submission.turn_id),
                vec![candidate(1, "primary", "model", 0.01)],
                1,
                cancel_rx,
            ))
            .await
            .unwrap_err();
        assert!(matches!(error, ModelCallError::Cancelled));

        let (cancel_tx, cancel_rx) = watch::channel(false);
        let executor_task = {
            let executor = executor.clone();
            let turn_id = submission.turn_id.clone();
            tokio::spawn(async move {
                executor
                    .execute(execution(
                        request("thread-1", &turn_id),
                        vec![candidate(1, "primary", "model", 0.01)],
                        1,
                        cancel_rx,
                    ))
                    .await
            })
        };
        started.notified().await;
        cancel_tx.send(true).unwrap();
        assert!(matches!(
            executor_task.await.unwrap().unwrap_err(),
            ModelCallError::Cancelled
        ));
        assert!(interrupted.load(Ordering::SeqCst));

        let events = service
            .events_after("thread-1", Some(&submission.cursor), 20)
            .unwrap()
            .events;
        assert!(!events
            .iter()
            .any(|row| row.event.event_type == OperatorEventType::RouteCompleted));
    }

    #[tokio::test]
    async fn missing_resolver_is_normalized_as_availability_without_send() {
        let (_evidence, service, submission) = service_and_turn();
        let executor = ModelCallExecutor::new(Arc::new(|_, _| None), service.clone());
        let (_cancel_tx, cancel_rx) = watch::channel(false);

        let error = executor
            .execute(execution(
                request("thread-1", &submission.turn_id),
                vec![candidate(1, "missing", "model", 0.01)],
                1,
                cancel_rx,
            ))
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            ModelCallError::AttemptsExhausted {
                attempts: 1,
                class: ProviderFailureClass::Availability,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn rate_auth_and_quota_errors_have_stable_normalized_classes() {
        let (_evidence, service, submission) = service_and_turn();
        let errors = HashMap::from([
            ("rate", "429 too many requests"),
            ("auth", "401 unauthorized"),
            ("quota", "quota exhausted"),
        ]);
        let resolver = Arc::new(move |provider: &str, _model: &str| {
            errors.get(provider).map(|message| {
                Arc::new(EventAdapter {
                    events: vec![StreamEvent::Error((*message).to_string())],
                }) as Arc<dyn ProviderAdapter>
            })
        });
        let executor = ModelCallExecutor::new(resolver, service.clone());
        let (_cancel_tx, cancel_rx) = watch::channel(false);

        let _ = executor
            .execute(execution(
                request("thread-1", &submission.turn_id),
                vec![
                    candidate(1, "rate", "m1", 0.01),
                    candidate(2, "auth", "m2", 0.02),
                    candidate(3, "quota", "m3", 0.03),
                ],
                3,
                cancel_rx,
            ))
            .await;

        let classes = service
            .events_after("thread-1", Some(&submission.cursor), 30)
            .unwrap()
            .events
            .into_iter()
            .filter(|row| row.event.event_type == OperatorEventType::RouteFailed)
            .map(|row| {
                row.event.payload["failure_class"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            classes,
            vec!["rate_limited", "authentication", "quota_exhausted"]
        );
    }

    #[tokio::test]
    async fn cost_over_budget_clamps_remaining_and_nonfinite_values_fail_safely() {
        let (_evidence, service, submission) = service_and_turn();
        let success = Arc::new(EventAdapter {
            events: vec![StreamEvent::Done(TokenUsage {
                cost_usd: 2.0,
                ..TokenUsage::default()
            })],
        }) as Arc<dyn ProviderAdapter>;
        let executor =
            ModelCallExecutor::new(Arc::new(move |_, _| Some(success.clone())), service.clone());
        let (_cancel_tx, cancel_rx) = watch::channel(false);
        let mut call = execution(
            request("thread-1", &submission.turn_id),
            vec![candidate(1, "primary", "model", 0.01)],
            1,
            cancel_rx,
        );
        call.remaining_budget_usd = Some(0.5);
        executor.execute(call).await.unwrap();
        let completed = service
            .events_after("thread-1", Some(&submission.cursor), 10)
            .unwrap()
            .events
            .into_iter()
            .find(|row| row.event.event_type == OperatorEventType::RouteCompleted)
            .unwrap();
        assert_eq!(completed.event.payload["remaining_budget_usd"], 0.0);

        let (_evidence, service, submission) = service_and_turn();
        let executor = ModelCallExecutor::new(Arc::new(|_, _| None), service);
        let (_cancel_tx, cancel_rx) = watch::channel(false);
        let mut call = execution(
            request("thread-1", &submission.turn_id),
            vec![candidate(1, "primary", "model", 0.01)],
            1,
            cancel_rx,
        );
        call.remaining_budget_usd = Some(f64::NAN);
        assert!(matches!(
            executor.execute(call).await.unwrap_err(),
            ModelCallError::InvalidBudget(_)
        ));

        let (_evidence, service, submission) = service_and_turn();
        let nonfinite = Arc::new(EventAdapter {
            events: vec![StreamEvent::Done(TokenUsage {
                cost_usd: f64::INFINITY,
                ..TokenUsage::default()
            })],
        }) as Arc<dyn ProviderAdapter>;
        let executor =
            ModelCallExecutor::new(Arc::new(move |_, _| Some(nonfinite.clone())), service);
        let (_cancel_tx, cancel_rx) = watch::channel(false);
        let error = executor
            .execute(execution(
                request("thread-1", &submission.turn_id),
                vec![candidate(1, "primary", "model", 0.01)],
                1,
                cancel_rx,
            ))
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            ModelCallError::AttemptsExhausted {
                class: ProviderFailureClass::InvalidUsage,
                ..
            }
        ));
    }
}
