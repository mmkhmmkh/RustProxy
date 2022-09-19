use std::borrow::{Borrow, BorrowMut, Cow};
use std::fmt;
use std::net::SocketAddr;
use std::string::FromUtf8Error;

use tokio::runtime;
use bytes::Bytes;
// use http_body_util::{combinators::BoxBody, BodyExt, Empty, Full};
use http_body::{combinators::BoxBody, Empty, Full};
use hyper::body::{Body, HttpBody};
use hyper::server::conn::Http;
use hyper::service::service_fn;
use hyper::{Client, Request, Response, StatusCode};
use tokio::net::TcpListener;
// use url::Url;
use hyper_tls::HttpsConnector;
use std::io::{Error, ErrorKind, Read, Write};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use url::Url;

const ORIGIN_PROTOCOL: &str = "http";

async fn handle_proxy(req: Request<Body>) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    let (mut parts, body) = req.into_parts();
    let headers_clone = parts.headers.clone();
    let origin = headers_clone.get("host").unwrap();

    let q = Url::parse(&*(ORIGIN_PROTOCOL.to_string() + "://" + origin.to_str().unwrap() + &parts.uri.to_string())).unwrap();
    println!("URI obj : {:?}", q);
    let params = q.query_pairs();
    let mut host: String = "".to_string();
    let mut prot: String = "https".to_string();
    for param in params {
        match param.0.to_string().as_str() {
            "origin" => {
                println!("ORIGIN: {}", param.1);
                host = param.1.to_string();
            }
            "protocol" => {
                prot = param.1.to_string();
            }
            _ => { }
        }
    };

    println!("Origin: {:?}", origin);
    println!("Host: {:?}", host);
    println!("Protocol: {:?}", prot);
    println!("URI: {:?}", (&*prot).to_owned() + "://" + &*host + parts.uri.path());

    parts.uri = ((&*prot).to_owned() + "://" + &*host + parts.uri.path())
        .parse()
        .unwrap();


    parts.headers.insert("x-forwarded-host", origin.to_owned());
    parts.headers.remove("host");
    parts.headers.insert("host", host.parse().unwrap());
    parts.headers.remove("accept");
    parts.headers.insert("accept", "*/*".parse().unwrap());
    parts.headers.remove("accept-encoding");
    parts.headers.insert("accept-encoding", "gzip".parse().unwrap());
    parts.headers.remove("accept-language");
    parts.headers.remove("upgrade-insecure-requests");

    println!("{:?}", parts);

    let mut resp = Client::builder()
        .build::<_, Body>(HttpsConnector::new())
        .request(Request::from_parts(parts, body))
        .await?;

    println!("Response: {}", resp.status());
    let mut resp_result = resp.into_parts();
    println!("{:?}", resp_result.0);
    if let Some(encoding) = resp_result.0.headers.get("content-encoding") {
        if encoding.to_str().unwrap().eq("gzip") {
            let bytes = hyper::body::to_bytes(resp_result.1).await?;
            let mut d = GzDecoder::new(&*bytes);
            let mut resp_body_str = String::new();
            d.read_to_string(&mut resp_body_str).unwrap();
            let resp_body_str = resp_body_str.replace((prot.to_owned() + "://" + &*host).as_str(), &*(ORIGIN_PROTOCOL.to_string() + "://" + origin.to_str().unwrap()));
            let resp_body_str = resp_body_str.replace("?", &*(ORIGIN_PROTOCOL.to_string() + "://" + origin.to_str().unwrap()));
            let mut e = GzEncoder::new(Vec::new(), Compression::default());
            e.write_all(resp_body_str.as_bytes()).unwrap();
            return Ok(Response::from_parts(resp_result.0, Body::from(e.finish().unwrap()).boxed()));
        } else {
            return Ok(Response::builder().status(501).body(Body::empty().boxed()).unwrap());
        }
    } else {
        return Ok(Response::from_parts(resp_result.0, resp_result.1.boxed()));
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    let listener = TcpListener::bind(addr).await?;
    println!("Listening on http://{}", addr);
    loop {
        let (stream, _) = listener.accept().await?;

        tokio::task::spawn(async move {
            if let Err(err) = Http::new().serve_connection(stream, service_fn(handle_proxy)).await {
                println!("Error serving connection: {:?}", err);
            }
        });
    }
}
