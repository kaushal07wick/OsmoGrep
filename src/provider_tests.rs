use super::*;
use std::{io::Write, net::TcpListener, thread, time::Instant};

#[test]
fn cancellation_closes_the_provider_socket() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let worker = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut reader = BufReader::new(socket.try_clone().unwrap());
        let mut line = String::new();
        let mut length = 0;
        loop {
            line.clear();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
            if line.to_ascii_lowercase().starts_with("content-length:") {
                length = line.split(':').nth(1).unwrap().trim().parse().unwrap();
            }
        }
        let mut body = vec![0; length];
        reader.read_exact(&mut body).unwrap();
        ready_tx.send(()).unwrap();
        let mut byte = [0];
        assert_eq!(
            socket.read(&mut byte).unwrap(),
            0,
            "provider connection should close after cancellation"
        );
    });
    let token = CancelToken::new();
    let other = token.clone();
    let cancel = thread::spawn(move || {
        ready_rx.recv().unwrap();
        other.cancel();
    });
    let cfg = ModelConfig {
        base_url: Some(format!("http://{address}")),
        ..Default::default()
    };
    let (tx, _rx) = mpsc::channel();
    assert!(request(&cfg, "fixture-key", &fixture(), &token, &tx).is_err());
    cancel.join().unwrap();
    worker.join().unwrap();
}

fn config(protocol: Api) -> ModelConfig {
    ModelConfig {
        api: Some(protocol),
        ..Default::default()
    }
}
fn call(id: &str) -> Value {
    json!({"type":"function_call","call_id":id,"name":"read_file","arguments":"{\"path\":\"hello.txt\"}"})
}
fn fixture() -> Value {
    json!({"model":"test","stream":true,"store":false,"tool_choice":"auto","input":[{"role":"system","content":"Help code"},{"role":"user","content":[{"type":"input_text","text":"Read café 🐛"},{"type":"input_image","image_url":"data:image/png;base64,AQID"}]}],"tools":[{"type":"function","name":"read_file","description":"Read","parameters":{"type":"object","properties":{"path":{"type":"string"}}}}]})
}

#[test]
fn protocol_defaults_and_custom_override_preserve_legacy_config() {
    let old: ModelConfig = toml::from_str("provider='groq'\nmodel='llama'\n").unwrap();
    assert_eq!(api(&old), Api::ChatCompletions);
    assert_eq!(api(&ModelConfig::default()), Api::Responses);
    assert_eq!(defaults("anthropic").1, "ANTHROPIC_API_KEY");
    let mut cfg = old;
    cfg.api = Some(Api::Responses);
    assert_eq!(
        api(&toml::from_str::<ModelConfig>(&toml::to_string(&cfg).unwrap()).unwrap()),
        Api::Responses
    );
    assert!(endpoint(&ModelConfig {
        base_url: Some("https://user:secret@host/v1".into()),
        ..cfg
    })
    .is_err());
    assert_eq!(defaults("custom").1, "OSMOGREP_API_KEY");
}

#[test]
fn protocols_convert_images_instructions_and_tools_without_openai_only_fields() {
    for protocol in [Api::ChatCompletions, Api::AnthropicMessages] {
        let body = payload(&config(protocol), &fixture()).unwrap();
        for field in ["store", "input", "reasoning"] {
            assert!(body.get(field).is_none());
        }
        let anthropic = protocol == Api::AnthropicMessages;
        let user = &body["messages"][if anthropic { 0 } else { 1 }]["content"];
        assert_eq!(user[0]["text"], "Read café 🐛");
        if anthropic {
            assert_eq!(body["system"][0]["text"], "Help code");
            assert_eq!(user[1]["source"]["data"], "AQID");
            assert_eq!(body["max_tokens"], 8192);
            assert!(body["tools"][0]["input_schema"].is_object());
        } else {
            assert_eq!(user[1]["image_url"]["url"], "data:image/png;base64,AQID");
            assert_eq!(body["tools"][0]["function"]["name"], "read_file");
            assert_eq!(body["stream_options"]["include_usage"], true);
        }
    }
}

#[test]
fn anthropic_signed_blocks_parallel_calls_and_cache_totals_round_trip() {
    let cfg = config(Api::AnthropicMessages);
    let blocks = json!([{"type":"thinking","thinking":"private","signature":"signed-A"},{"type":"tool_use","id":"a","name":"read_file","input":{"path":"a"}},{"type":"thinking","thinking":"more","signature":"signed-B"},{"type":"tool_use","id":"b","name":"read_file","input":{"path":"b"}}]);
    let out=normalized(&cfg,json!({"content":blocks,"stop_reason":"tool_use","usage":{"input_tokens":10,"cache_read_input_tokens":20,"cache_creation_input_tokens":5,"output_tokens":3}})).unwrap();
    assert_eq!(out["usage"]["input_tokens"], 35);
    assert_eq!(out["usage"]["total_tokens"], 38);
    let mut input = vec![
        out["output"][0].clone(),
        call("a"),
        json!({"type":"function_call_output","call_id":"a","output":"A"}),
        call("b"),
        json!({"type":"function_call_output","call_id":"b","output":"B"}),
    ];
    let body = payload(&cfg, &json!({"input":input})).unwrap();
    assert_eq!(body["messages"].as_array().unwrap().len(), 2);
    assert_eq!(body["messages"][0]["content"], blocks);
    assert_eq!(body["messages"][1]["content"].as_array().unwrap().len(), 2);
    assert_eq!(body["messages"][1]["content"][1]["tool_use_id"], "b");
    input.push(json!({"role":"user","content":"Continue"}));
    assert_eq!(
        payload(&cfg, &json!({"input":input})).unwrap()["messages"][1]["content"][2]["text"],
        "Continue"
    );
}

#[test]
fn response_only_fields_do_not_poison_the_next_request() {
    let out=normalized(&config(Api::AnthropicMessages),json!({"stop_reason":"tool_use","content":[{"type":"text","text":"Reading","citations":null,"parsed_output":null},{"type":"tool_use","id":"a","name":"read_file","input":{},"caller":{"type":"direct"}}]})).unwrap();
    let native = &out["output"][0]["osmogrep_replay"]["message"]["content"];
    assert_eq!(native[0], json!({"type":"text","text":"Reading"}));
    assert!(native[1].get("caller").is_none());
    let chat=normalized(&config(Api::ChatCompletions),json!({"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"Done","refusal":null,"annotations":[],"audio":null}}]})).unwrap();
    assert_eq!(
        chat["output"][0]["osmogrep_replay"]["message"],
        json!({"role":"assistant","content":"Done"})
    );
}

#[test]
fn chat_parallel_calls_replay_reasoning_and_do_not_duplicate_calls() {
    let cfg = config(Api::ChatCompletions);
    let message = json!({"role":"assistant","content":"Checking","reasoning_content":"plan","tool_calls":[{"id":"a","type":"function","function":{"name":"read_file","arguments":"{}"}},{"id":"b","type":"function","function":{"name":"read_file","arguments":"{}"},"extra_content":{"google":{"thought_signature":"opaque"}}}]});
    let out = normalized(
        &cfg,
        json!({"choices":[{"message":message,"finish_reason":"tool_calls"}]}),
    )
    .unwrap();
    let body=payload(&cfg,&json!({"input":[out["output"][0],out["output"][1],call("a"),{"type":"function_call_output","call_id":"a","output":"A"},call("b"),{"type":"function_call_output","call_id":"b","output":"B"}]})).unwrap();
    assert_eq!(body["messages"].as_array().unwrap().len(), 3);
    assert_eq!(body["messages"][0], message);
    assert_eq!(body["messages"][2]["tool_call_id"], "b");
}

#[test]
fn fragmented_chat_tools_are_assembled_by_index_and_usage_survives_final_chunk() {
    let mut s = StreamState::new(Api::ChatCompletions);
    for e in [
        json!({"choices":[{"delta":{"reasoning_content":"plan","tool_calls":[{"index":1,"id":"b","function":{"name":"read_","arguments":"{\"path\":"}}]}}]}),
        json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"a","function":{"name":"list_dir","arguments":"{}"}},{"index":1,"function":{"name":"file","arguments":"\"é.txt\"}"}}]},"finish_reason":"tool_calls"}]}),
        json!({"choices":[],"usage":{"prompt_tokens":7,"completion_tokens":3,"total_tokens":10}}),
    ] {
        s.event(Api::ChatCompletions, e).unwrap();
    }
    s.finished = true;
    let out = normalized(
        &config(Api::ChatCompletions),
        s.finish(Api::ChatCompletions).unwrap(),
    )
    .unwrap();
    assert_eq!(out["output"][2]["name"], "read_file");
    assert_eq!(out["output"][2]["arguments"], "{\"path\":\"é.txt\"}");
    assert_eq!(out["usage"]["total_tokens"], 10);
}

#[test]
fn anthropic_stream_preserves_signatures_and_initial_usage() {
    let mut s = StreamState::new(Api::AnthropicMessages);
    for e in [
        json!({"type":"message_start","message":{"usage":{"input_tokens":9,"cache_read_input_tokens":2}}}),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"","signature":""}}),
        json!({"type":"content_block_delta","index":0,"delta":{"thinking":"plan"}}),
        json!({"type":"content_block_delta","index":0,"delta":{"signature":"signed"}}),
        json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"a","name":"read_file","input":{}}}),
        json!({"type":"content_block_delta","index":1,"delta":{"partial_json":"{\"path\":\"a\"}"}}),
        json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":5}}),
        json!({"type":"message_stop"}),
    ] {
        s.event(Api::AnthropicMessages, e).unwrap();
    }
    let out = normalized(
        &config(Api::AnthropicMessages),
        s.finish(Api::AnthropicMessages).unwrap(),
    )
    .unwrap();
    assert_eq!(
        out["output"][0]["osmogrep_replay"]["message"]["content"][0]["signature"],
        "signed"
    );
    assert_eq!(out["usage"]["input_tokens"], 11);
    assert_eq!(out["usage"]["output_tokens"], 5);
}

#[test]
fn truncated_and_invalid_calls_are_rejected_before_dispatch() {
    assert!(normalized(
        &config(Api::Responses),
        json!({"status":"completed","error":null,"output":[{"type":"output_text","text":"Hello"}]})
    )
    .is_ok());
    assert!(normalized(&config(Api::ChatCompletions),json!({"choices":[{"finish_reason":"length","message":{"tool_calls":[{"id":"a","function":{"name":"write_file","arguments":"{}"}}]}}]})).is_err());
    assert!(normalized(
        &config(Api::Responses),
        json!({"status":"incomplete","output":[call("a")]})
    )
    .is_err());
    assert!(normalized(
        &config(Api::Responses),
        json!({"output":[call("a"),call("a")]})
    )
    .is_err());
    assert!(normalized(&config(Api::Responses),json!({"output":[call("a"),{"type":"function_call","call_id":"b","name":"write_file","arguments":"{"}]})).is_err());
    assert!(StreamState::new(Api::ChatCompletions)
        .finish(Api::ChatCompletions)
        .is_err());
}

fn server(
    body: String,
    content_type: &str,
    status: u16,
    fragment: bool,
) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let ct = content_type.to_string();
    let join = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut reader = BufReader::new(socket.try_clone().unwrap());
        let mut headers = String::new();
        let mut length = 0;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
            if let Some((k, v)) = line.split_once(':') {
                if k.eq_ignore_ascii_case("content-length") {
                    length = v.trim().parse().unwrap();
                }
            }
            headers.push_str(&line);
        }
        let mut bytes = vec![0; length];
        reader.read_exact(&mut bytes).unwrap();
        headers.push_str(std::str::from_utf8(&bytes).unwrap());
        write!(socket,"HTTP/1.1 {status} Test\r\nContent-Type: {ct}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).unwrap();
        if fragment {
            for byte in body.bytes() {
                socket.write_all(&[byte]).unwrap();
            }
        } else {
            socket.write_all(body.as_bytes()).unwrap();
        }
        headers
    });
    (format!("http://{addr}/v1"), join)
}

#[test]
fn real_http_auth_endpoints_json_and_sse_utf8_are_protocol_specific() {
    for protocol in [Api::Responses, Api::ChatCompletions, Api::AnthropicMessages] {
        let raw = match protocol {
            Api::Responses => json!({"output":[{"type":"output_text","text":"hé 🐛"}]}),
            Api::ChatCompletions => {
                json!({"choices":[{"message":{"role":"assistant","content":"hé 🐛"},"finish_reason":"stop"}],"usage":{"prompt_tokens":4,"completion_tokens":2}})
            }
            Api::AnthropicMessages => {
                json!({"content":[{"type":"text","text":"hé 🐛"}],"stop_reason":"end_turn","usage":{"input_tokens":4,"output_tokens":2}})
            }
        };
        let (url, join) = server(raw.to_string(), "application/json", 200, false);
        let cfg = ModelConfig {
            base_url: Some(url),
            ..config(protocol)
        };
        let (tx, _) = mpsc::channel();
        let (value, streamed) =
            request(&cfg, "secret", &fixture(), &CancelToken::new(), &tx).unwrap();
        assert!(!streamed);
        assert!(value.to_string().contains("hé 🐛"));
        let wire = join.join().unwrap();
        let path = match protocol {
            Api::Responses => "/v1/responses",
            Api::ChatCompletions => "/v1/chat/completions",
            Api::AnthropicMessages => "/v1/messages",
        };
        assert!(wire.starts_with(&format!("POST {path} ")));
        if protocol == Api::AnthropicMessages {
            assert!(wire.contains("x-api-key: secret"));
            assert!(wire.contains("anthropic-version:"));
            assert!(!wire.contains("authorization:"));
        } else {
            assert!(wire.contains("authorization: Bearer secret"));
        }
    }
    let data = format!(
        "data:{}\r\n\r\ndata: {}\r\n\r\ndata:[DONE]\r\n\r\n",
        json!({"choices":[{"delta":{"content":"hé 🐛"},"finish_reason":"stop"}]}),
        json!({"choices":[],"usage":{"prompt_tokens":4,"completion_tokens":2}})
    );
    let (url, join) = server(data, "text/event-stream", 200, true);
    let (tx, rx) = mpsc::channel();
    let (value, streamed) = request(
        &ModelConfig {
            base_url: Some(url),
            ..config(Api::ChatCompletions)
        },
        "secret",
        &fixture(),
        &CancelToken::new(),
        &tx,
    )
    .unwrap();
    assert!(streamed);
    assert_eq!(value["usage"]["prompt_tokens"], 4);
    assert!(rx
        .try_iter()
        .any(|e| matches!(e,AgentEvent::StreamDelta(s) if s=="hé 🐛")));
    join.join().unwrap();
}

#[test]
fn authentication_errors_do_not_retry_or_expose_keys() {
    for (status, retry) in [(401, false), (429, true), (503, true)] {
        let (url, join) = server(
            json!({"error":{"message":"bad secret"}}).to_string(),
            "application/json",
            status,
            false,
        );
        let (tx, _) = mpsc::channel();
        let err = request(
            &ModelConfig {
                base_url: Some(url),
                ..Default::default()
            },
            "secret",
            &fixture(),
            &CancelToken::new(),
            &tx,
        )
        .unwrap_err();
        assert_eq!(err.retryable, retry);
        assert!(!err.message.contains("secret"));
        join.join().unwrap();
    }
}

#[test]
fn cancellation_returns_while_provider_is_silent() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let _connection = listener.accept().unwrap();
        thread::sleep(Duration::from_millis(400));
    });
    let cancel = CancelToken::new();
    let token = cancel.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(75));
        token.cancel();
    });
    let (tx, _) = mpsc::channel();
    let now = Instant::now();
    let err = request(
        &ModelConfig {
            base_url: Some(format!("http://{address}")),
            ..Default::default()
        },
        "secret",
        &fixture(),
        &cancel,
        &tx,
    )
    .unwrap_err();
    assert_eq!(err.message, "cancelled");
    assert!(now.elapsed() < Duration::from_millis(300));
    server.join().unwrap();
}
