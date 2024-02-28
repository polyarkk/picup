use std::collections::HashMap;

use clap::{arg, command};
use reqwest::{blocking::{multipart::Form, Client}, StatusCode};

fn main() {
    let matches = command!()
        .args(&[
            arg!(-t --tokenn <token>    "Token for access to uploading images to the picbed.")
                .required(true),
            arg!(-u --apiurl <api_url>  "\"/upload\" api url prefix for PicUp server. Leave empty if you use it locally."),
            arg!([images]               "File paths for images to be uploaded.")
                .required(true)
                .num_args(0..),
        ])
        .get_matches();

    let token = matches.get_one::<String>("token").expect("no token");

    let api_url = match matches.get_one::<String>("apiurl") {
        Some(api_url) => api_url.to_owned(),
        None => format!("http://127.0.0.1/picup"),
    };

    let paths = matches.get_many::<String>("images").unwrap();

    let client = Client::new();

    let form = {
        let mut form = Form::new();

        for path in paths {
            form = form.file("file", path).expect("invalid file path");
        }

        form
    };
    
    let res = client.post(format!("{}/upload", api_url))
        .query(&[("access_token", token)])
        .multipart(form)
        .send()
        .unwrap();

    if res.status() != StatusCode::OK {
        
    }
}
