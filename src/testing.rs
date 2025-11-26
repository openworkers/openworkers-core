//! Generic test utilities for OpenWorkers runtimes
//!
//! This module provides macros to generate standard tests for any runtime
//! that implements the `Worker` trait.
//!
//! # Usage
//!
//! In your runtime's test file:
//!
//! ```ignore
//! use openworkers_core::generate_worker_tests;
//! use your_runtime::Worker;
//!
//! generate_worker_tests!(Worker);
//! ```

/// Generate standard worker tests for a runtime
///
/// This macro generates a comprehensive test suite for any Worker implementation.
/// Tests cover: basic responses, JSON, custom status, request properties, async handlers, etc.
#[macro_export]
macro_rules! generate_worker_tests {
    ($worker:ty) => {
        use std::collections::HashMap;
        use $crate::{HttpRequest, Script, Task, Worker as _};

        #[tokio::test]
        async fn test_simple_response() {
            let script = r#"
                addEventListener('fetch', (event) => {
                    event.respondWith(new Response('Hello, World!'));
                });
            "#;

            let script_obj = Script::new(script);
            let mut worker = <$worker>::new(script_obj, None, None)
                .await
                .expect("Worker should initialize");

            let request = HttpRequest {
                method: "GET".to_string(),
                url: "http://localhost/".to_string(),
                headers: HashMap::new(),
                body: None,
            };

            let (task, rx) = Task::fetch(request);
            worker.exec(task).await.expect("Task should execute");

            let response = rx.await.expect("Should receive response");
            assert_eq!(response.status, 200);

            let body = response.body.as_bytes().expect("Should have body");
            assert_eq!(String::from_utf8_lossy(body), "Hello, World!");
        }

        #[tokio::test]
        async fn test_json_response() {
            let script = r#"
                addEventListener('fetch', (event) => {
                    const data = { message: 'Hello', value: 42 };
                    event.respondWith(new Response(JSON.stringify(data), {
                        headers: { 'Content-Type': 'application/json' }
                    }));
                });
            "#;

            let script_obj = Script::new(script);
            let mut worker = <$worker>::new(script_obj, None, None)
                .await
                .expect("Worker should initialize");

            let request = HttpRequest {
                method: "GET".to_string(),
                url: "http://localhost/".to_string(),
                headers: HashMap::new(),
                body: None,
            };

            let (task, rx) = Task::fetch(request);
            worker.exec(task).await.expect("Task should execute");

            let response = rx.await.expect("Should receive response");
            assert_eq!(response.status, 200);

            let content_type = response
                .headers
                .iter()
                .find(|(k, _)| k.to_lowercase() == "content-type")
                .map(|(_, v)| v.as_str());
            assert_eq!(content_type, Some("application/json"));

            let body = response.body.as_bytes().expect("Should have body");
            let body_str = String::from_utf8_lossy(body);
            assert!(
                body_str.contains("Hello"),
                "Body should contain 'Hello': {}",
                body_str
            );
            assert!(
                body_str.contains("42"),
                "Body should contain '42': {}",
                body_str
            );
        }

        #[tokio::test]
        async fn test_custom_status() {
            let script = r#"
                addEventListener('fetch', (event) => {
                    event.respondWith(new Response('Not Found', { status: 404 }));
                });
            "#;

            let script_obj = Script::new(script);
            let mut worker = <$worker>::new(script_obj, None, None)
                .await
                .expect("Worker should initialize");

            let request = HttpRequest {
                method: "GET".to_string(),
                url: "http://localhost/".to_string(),
                headers: HashMap::new(),
                body: None,
            };

            let (task, rx) = Task::fetch(request);
            worker.exec(task).await.expect("Task should execute");

            let response = rx.await.expect("Should receive response");
            assert_eq!(response.status, 404);
        }

        #[tokio::test]
        async fn test_request_method() {
            let script = r#"
                addEventListener('fetch', (event) => {
                    const method = event.request.method;
                    event.respondWith(new Response(method));
                });
            "#;

            let script_obj = Script::new(script);
            let mut worker = <$worker>::new(script_obj, None, None)
                .await
                .expect("Worker should initialize");

            let request = HttpRequest {
                method: "POST".to_string(),
                url: "http://localhost/".to_string(),
                headers: HashMap::new(),
                body: None,
            };

            let (task, rx) = Task::fetch(request);
            worker.exec(task).await.expect("Task should execute");

            let response = rx.await.expect("Should receive response");
            let body = response.body.as_bytes().expect("Should have body");
            assert_eq!(String::from_utf8_lossy(body), "POST");
        }

        #[tokio::test]
        async fn test_request_url() {
            let script = r#"
                addEventListener('fetch', (event) => {
                    const url = new URL(event.request.url);
                    event.respondWith(new Response(url.pathname));
                });
            "#;

            let script_obj = Script::new(script);
            let mut worker = <$worker>::new(script_obj, None, None)
                .await
                .expect("Worker should initialize");

            let request = HttpRequest {
                method: "GET".to_string(),
                url: "http://localhost/api/test".to_string(),
                headers: HashMap::new(),
                body: None,
            };

            let (task, rx) = Task::fetch(request);
            worker.exec(task).await.expect("Task should execute");

            let response = rx.await.expect("Should receive response");
            let body = response.body.as_bytes().expect("Should have body");
            assert_eq!(String::from_utf8_lossy(body), "/api/test");
        }

        #[tokio::test]
        async fn test_async_handler() {
            let script = r#"
                addEventListener('fetch', (event) => {
                    event.respondWith(
                        Promise.resolve({ async: true })
                            .then(data => new Response(JSON.stringify(data)))
                    );
                });
            "#;

            let script_obj = Script::new(script);
            let mut worker = <$worker>::new(script_obj, None, None)
                .await
                .expect("Worker should initialize");

            let request = HttpRequest {
                method: "GET".to_string(),
                url: "http://localhost/".to_string(),
                headers: HashMap::new(),
                body: None,
            };

            let (task, rx) = Task::fetch(request);
            worker.exec(task).await.expect("Task should execute");

            let response = rx.await.expect("Should receive response");
            let body = response.body.as_bytes().expect("Should have body");
            let body_str = String::from_utf8_lossy(body);
            assert!(
                body_str.contains("true"),
                "Body should contain async:true: {}",
                body_str
            );
        }

        #[tokio::test]
        async fn test_response_headers() {
            let script = r#"
                addEventListener('fetch', (event) => {
                    event.respondWith(new Response('OK', {
                        status: 201,
                        headers: {
                            'X-Custom-Header': 'custom-value',
                            'X-Another': 'another-value'
                        }
                    }));
                });
            "#;

            let script_obj = Script::new(script);
            let mut worker = <$worker>::new(script_obj, None, None)
                .await
                .expect("Worker should initialize");

            let request = HttpRequest {
                method: "GET".to_string(),
                url: "http://localhost/".to_string(),
                headers: HashMap::new(),
                body: None,
            };

            let (task, rx) = Task::fetch(request);
            worker.exec(task).await.expect("Task should execute");

            let response = rx.await.expect("Should receive response");
            assert_eq!(response.status, 201);

            let custom_header = response
                .headers
                .iter()
                .find(|(k, _)| k.to_lowercase() == "x-custom-header")
                .map(|(_, v)| v.as_str());
            assert_eq!(custom_header, Some("custom-value"));
        }

        #[tokio::test]
        async fn test_empty_response() {
            let script = r#"
                addEventListener('fetch', (event) => {
                    event.respondWith(new Response(null, { status: 204 }));
                });
            "#;

            let script_obj = Script::new(script);
            let mut worker = <$worker>::new(script_obj, None, None)
                .await
                .expect("Worker should initialize");

            let request = HttpRequest {
                method: "GET".to_string(),
                url: "http://localhost/".to_string(),
                headers: HashMap::new(),
                body: None,
            };

            let (task, rx) = Task::fetch(request);
            worker.exec(task).await.expect("Task should execute");

            let response = rx.await.expect("Should receive response");
            assert_eq!(response.status, 204);
        }

        #[tokio::test]
        async fn test_console_log() {
            let script = r#"
                addEventListener('fetch', (event) => {
                    console.log('Log message');
                    event.respondWith(new Response('logged'));
                });
            "#;

            let script_obj = Script::new(script);
            let mut worker = <$worker>::new(script_obj, None, None)
                .await
                .expect("Worker should initialize");

            let request = HttpRequest {
                method: "GET".to_string(),
                url: "http://localhost/".to_string(),
                headers: HashMap::new(),
                body: None,
            };

            let (task, rx) = Task::fetch(request);
            worker.exec(task).await.expect("Task should execute");

            let response = rx.await.expect("Should receive response");
            assert_eq!(response.status, 200);
        }

        #[tokio::test]
        async fn test_worker_creation_error() {
            let script = r#"
                this is not valid javascript
            "#;

            let script_obj = Script::new(script);
            let result = <$worker>::new(script_obj, None, None).await;
            assert!(
                result.is_err(),
                "Invalid script should fail to create worker"
            );
        }

        #[tokio::test]
        async fn test_multiple_requests() {
            let script = r#"
                let counter = 0;
                addEventListener('fetch', (event) => {
                    counter++;
                    event.respondWith(new Response('Request ' + counter));
                });
            "#;

            let script_obj = Script::new(script);
            let mut worker = <$worker>::new(script_obj, None, None)
                .await
                .expect("Worker should initialize");

            for i in 1..=3 {
                let request = HttpRequest {
                    method: "GET".to_string(),
                    url: "http://localhost/".to_string(),
                    headers: HashMap::new(),
                    body: None,
                };

                let (task, rx) = Task::fetch(request);
                worker.exec(task).await.expect("Task should execute");

                let response = rx.await.expect("Should receive response");
                let body = response.body.as_bytes().expect("Should have body");
                assert_eq!(String::from_utf8_lossy(body), format!("Request {}", i));
            }
        }
    };
}

/// Generate benchmark functions for a runtime
///
/// This macro generates standard benchmarks using Criterion.
/// Usage in benches/worker_benchmark.rs:
///
/// ```ignore
/// use criterion::{criterion_group, criterion_main, Criterion};
/// use openworkers_core::generate_worker_benchmarks;
/// use your_runtime::Worker;
///
/// generate_worker_benchmarks!(Worker);
///
/// criterion_group!(benches, worker_benchmarks);
/// criterion_main!(benches);
/// ```
#[macro_export]
macro_rules! generate_worker_benchmarks {
    ($worker:ty) => {
        use $crate::{HttpRequest, Script, Task, Worker as _};
        use std::collections::HashMap;

        pub fn worker_benchmarks(c: &mut Criterion) {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            let mut group = c.benchmark_group("Worker");

            group.bench_function("new", |b| {
                b.iter(|| {
                    rt.block_on(async {
                        let script = Script::new(
                            r#"addEventListener('fetch', (e) => e.respondWith(new Response('OK')));"#,
                        );
                        <$worker>::new(script, None, None).await.unwrap()
                    })
                })
            });

            group.bench_function("exec_simple_response", |b| {
                let script = Script::new(
                    r#"addEventListener('fetch', (e) => e.respondWith(new Response('OK')));"#,
                );
                let mut worker = rt.block_on(<$worker>::new(script, None, None)).unwrap();

                b.iter(|| {
                    rt.block_on(async {
                        let req = HttpRequest {
                            method: "GET".to_string(),
                            url: "http://localhost/".to_string(),
                            headers: HashMap::new(),
                            body: None,
                        };
                        let (task, rx) = Task::fetch(req);
                        worker.exec(task).await.unwrap();
                        rx.await.unwrap()
                    })
                })
            });

            group.bench_function("exec_json_response", |b| {
                let script = Script::new(
                    r#"addEventListener('fetch', (e) => e.respondWith(new Response(JSON.stringify({a:1,b:2}))));"#,
                );
                let mut worker = rt.block_on(<$worker>::new(script, None, None)).unwrap();

                b.iter(|| {
                    rt.block_on(async {
                        let req = HttpRequest {
                            method: "GET".to_string(),
                            url: "http://localhost/".to_string(),
                            headers: HashMap::new(),
                            body: None,
                        };
                        let (task, rx) = Task::fetch(req);
                        worker.exec(task).await.unwrap();
                        rx.await.unwrap()
                    })
                })
            });

            group.bench_function("exec_with_headers", |b| {
                let script = Script::new(
                    r#"addEventListener('fetch', (e) => e.respondWith(new Response('OK', {headers: {'X-A': '1', 'X-B': '2'}})));"#,
                );
                let mut worker = rt.block_on(<$worker>::new(script, None, None)).unwrap();

                b.iter(|| {
                    rt.block_on(async {
                        let req = HttpRequest {
                            method: "GET".to_string(),
                            url: "http://localhost/".to_string(),
                            headers: HashMap::new(),
                            body: None,
                        };
                        let (task, rx) = Task::fetch(req);
                        worker.exec(task).await.unwrap();
                        rx.await.unwrap()
                    })
                })
            });

            group.finish();
        }
    };
}
