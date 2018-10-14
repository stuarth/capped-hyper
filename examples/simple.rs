extern crate capped_hyper;
extern crate futures;
extern crate hyper;
extern crate tokio_core;

// use capped_hyper::Client;
use capped_hyper as ch;
use futures::{future, Future, Stream};
use hyper::{Body, Client, Error, Request};
use std::io::{self, Write};
use tokio_core::reactor::Core;

pub fn main() -> Result<(), Error> {
    let mut core = Core::new().unwrap();
    let client = Client::new();
    let capped = ch::CappedClient::new(client, 2);

    let uris = vec![
        "http://worldclockapi.com/api/json/utc/now",
        "http://worldclockapi.com/api/json/utc/now",
        "http://worldclockapi.com/api/json/utc/now",
    ];
    let work = uris.into_iter().map(|uri| {
        let uri: hyper::Uri = uri.parse().unwrap();
        let req = Request::get(uri).body(Body::empty()).expect("req");
        capped.request(req).and_then(move |res| {
            println!("Response: {}", &res.status());

            res.into_body()
                .for_each(|chunk| io::stdout().write_all(&chunk).map_err(|_e| panic!(":(")))
        })
    });

    let _ = core.run(future::join_all(work));

    Ok(())
}
