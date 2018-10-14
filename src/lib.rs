extern crate futures;
extern crate hyper;

use futures::{
    prelude::*,
    task::{self, Task},
};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

pub struct CappedClient<C> {
    client: Client<C>,
    max: u32,
    state: State,
}
pub use hyper::{
    body::Body,
    client::{connect::Connect, ResponseFuture},
    Client, Request, Response,
};

#[derive(Clone)]
struct State(Arc<Mutex<ClientState>>);

impl Default for State {
    fn default() -> Self {
        State(Arc::new(Mutex::new(ClientState {
            in_flight: 0,
            queue: VecDeque::new(),
        })))
    }
}

struct ClientState {
    in_flight: u32,
    queue: VecDeque<Task>,
}

impl ClientState {
    fn queue_task(&mut self, task: Task) {
        self.queue.push_back(task);
    }

    fn notify_next(&mut self) {
        if let Some(task) = self.queue.pop_front() {
            task.notify();
        }
    }
}

pub struct CappedFuture {
    inner: ResponseFuture,
    state: State,
    max: u32,
    queued: bool,
}

impl Future for CappedFuture {
    type Item = Response<Body>;
    type Error = hyper::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if self.queued {
            let mut s = match self.state.0.try_lock() {
                Ok(s) => s,
                Err(_) => {
                    return Ok(Async::NotReady);
                }
            };

            let current = s.in_flight;
            if current == self.max {
                s.queue_task(task::current());
                return Ok(Async::NotReady);
            } else {
                self.queued = false;
                s.in_flight += 1;
            }
        }

        match self.inner.poll() {
            Ok(Async::Ready(response)) => {
                if let Ok(mut s) = self.state.0.try_lock() {
                    s.in_flight -= 1;
                    s.notify_next();
                    Ok(Async::Ready(response))
                } else {
                    Ok(Async::NotReady)
                }
            }
            other @ _ => other,
        }
    }
}

impl<C: Connect + 'static> CappedClient<C> {
    pub fn new(client: hyper::Client<C>, max: u32) -> Self {
        CappedClient {
            client,
            max,
            state: State::default(),
        }
    }

    pub fn request(&self, req: Request<Body>) -> CappedFuture {
        let inner = self.client.request(req);

        CappedFuture {
            inner,
            state: self.state.clone(),
            max: self.max,
            queued: true,
        }
    }
}
