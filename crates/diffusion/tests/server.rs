//! Runs against a real ComfyUI server with the Anima models and the add-ons: ignored by default,
//! run with `PROCRAFT_COMFY_URL=host:port cargo test -p procraft-diffusion -- --ignored`
//! (`127.0.0.1:8189`, the development server, if it isn't set).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::{Duration, Instant};

use procraft_diffusion::{
    Address, Connection, Error, Family, Feature, Request, Resolution, RunState, ServerState, Worker, plan_for,
};

fn address() -> Address {
    let text = std::env::var("PROCRAFT_COMFY_URL").unwrap_or_else(|_| "127.0.0.1:8189".to_owned());
    Address::parse(&text).unwrap()
}

fn connected(worker: &Worker) -> ServerState {
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        let server = worker.server();
        match &server.connection {
            Connection::Connected => return server,
            Connection::Failed(e) => panic!("can't connect: {e}"),
            _ if Instant::now() > until => panic!("connecting took too long"),
            _ => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}

fn wait(worker: &Worker, id: u64, until: impl Fn(&RunState) -> bool) -> RunState {
    let limit = Instant::now() + Duration::from_secs(300);
    loop {
        let run = worker.run(id).unwrap();
        if until(&run) || run.finished || Instant::now() > limit {
            return run;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn turbo(server: &ServerState, size: (u32, u32), count: u32) -> Request {
    let style = server.styles().into_iter().find(|s| s.id == "anima-turbo").expect("the turbo checkpoint");
    Request {
        prompt: "1girl, scenery, sky".into(),
        style,
        strength: 1.0,
        seed: 1234,
        count,
        picture: None,
        size,
        resolution: Resolution::Quality,
        largest: Some(6.0),
        nsfw: false,
    }
}

#[test]
#[ignore = "needs a ComfyUI server"]
fn the_server_is_ready_for_anima() {
    let worker = Worker::new();
    worker.set_address(Some(address()));
    let server = connected(&worker);
    for feature in Feature::ALL {
        assert!(server.ready(Family::Anima, feature), "not ready for {feature:?}");
    }
}

#[test]
#[ignore = "needs a ComfyUI server"]
fn pictures_come_back_at_the_size_asked() {
    let worker = Worker::new();
    worker.set_address(Some(address()));
    let server = connected(&worker);
    let id = worker.submit(turbo(&server, (700, 500), 2));
    let run = wait(&worker, id, |_| false);
    assert_eq!(run.error, None);
    assert_eq!(run.results.len(), 2);
    assert_eq!(run.results.iter().map(|r| r.seed).collect::<Vec<_>>(), [1234, 1235]);
    let plan = plan_for(700, 500, Family::Anima.native_pixels(), Resolution::Quality, 6.0, false);
    for r in &run.results {
        assert_eq!((r.picture.width, r.picture.height), (700, 500));
        assert_eq!(r.generated, (plan.size.width, plan.size.height));
    }
}

#[test]
#[ignore = "needs a ComfyUI server"]
fn a_run_can_be_cancelled_while_it_samples() {
    let worker = Worker::new();
    worker.set_address(Some(address()));
    let server = connected(&worker);
    let mut request = turbo(&server, (1024, 1024), 3);
    request.style = server.styles().into_iter().find(|s| s.id == "anima").unwrap();
    request.seed = Instant::now().elapsed().as_nanos() as u64 + 99;
    let id = worker.submit(request);
    wait(&worker, id, |r| r.progress > 0.05);
    let asked = Instant::now();
    worker.cancel(id);
    let run = wait(&worker, id, |_| false);
    assert_eq!(run.error, Some(Error::Cancelled));
    assert!(run.results.len() < 3);
    assert!(asked.elapsed() < Duration::from_secs(5), "took {:?}", asked.elapsed());
}

#[test]
#[ignore = "needs a ComfyUI server"]
fn a_wrong_address_says_so() {
    let worker = Worker::new();
    worker.set_address(Address::parse("127.0.0.1:9"));
    let until = Instant::now() + Duration::from_secs(15);
    loop {
        match worker.server().connection {
            Connection::Failed(Error::Unreachable { address, .. }) => {
                assert_eq!(address, "127.0.0.1:9");
                return;
            }
            Connection::Failed(e) => panic!("{e}"),
            _ if Instant::now() > until => panic!("no answer"),
            _ => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}
