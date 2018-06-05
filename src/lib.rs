extern crate futures;
extern crate hyper;

use futures::{
    prelude::*, task::{self, Task},
};
use hyper::{
    body::Body, client::{connect::Connect, ResponseFuture}, Request, Response,
};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};

pub struct Client<C> {
    client: hyper::Client<C>,
    max: u32,
    state: State,
}

#[derive(Clone)]
struct State(Rc<RefCell<ClientState>>);

impl Default for State {
    fn default() -> Self {
        State(Rc::new(RefCell::new(ClientState {
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
            let mut s = self.state.0.borrow_mut();
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
                let mut s = self.state.0.borrow_mut();
                s.in_flight -= 1;
                s.notify_next();
                Ok(Async::Ready(response))
            }
            other @ _ => other,
        }
    }
}

impl<C: Connect + 'static> Client<C> {
    pub fn new(client: hyper::Client<C>, max: u32) -> Self {
        Client {
            client,
            max,
            state: State::default(),
        }
    }

    pub fn request(&mut self, req: Request<Body>) -> CappedFuture {
        let inner = self.client.request(req);

        CappedFuture {
            inner,
            state: self.state.clone(),
            max: self.max,
            queued: true,
        }
    }
}
