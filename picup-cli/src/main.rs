use clap::{arg, command};
use picup_lib::{picup, Result};

fn main() -> Result<()> {
    let mut matches = command!()
        .args(&[
            arg!(-c --category <category>   "Category uploading the images to.")
                .required(true),
            arg!(-t --token <token>         "Token for access to uploading images to the server.")
                .required(true),
            arg!(-u --"api-url" <url>       "\"/upload\" api url prefix for PicUp server. Default: http://127.0.0.1:19190"),
            arg!([images]                   "File paths for images to be uploaded.")
                .required(true)
                .num_args(0..),
        ])
        .get_matches();

    let category = matches.remove_one::<String>("category").unwrap();

    let token = matches.remove_one::<String>("token").unwrap();

    let api_url = match matches.remove_one::<String>("api-url") {
        Some(api_url) => api_url,
        None => "http://127.0.0.1:19190".to_string(),
    };

    let paths = matches
        .remove_many::<String>("images")
        .unwrap()
        .collect::<Vec<String>>();

    let urls = picup(&api_url, &token, &category, &paths)?;

    for url in urls {
        println!("{}", url);
    }

    Ok(())
}
