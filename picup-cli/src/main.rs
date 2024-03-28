use clap::{arg, command, ArgAction};
use picup_lib::{picup, Result, UploadImgParam};

fn main() -> Result<()> {
    let mut matches = command!()
        .args(&[
            arg!(-o --"override"            "Override existing images in the server.")
                .action(ArgAction::SetTrue),
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

    let api_url = matches
        .remove_one::<String>("api-url")
        .unwrap_or("http://127.0.0.1:19190".to_string());

    let paths = matches
        .remove_many::<String>("images")
        .unwrap()
        .collect::<Vec<String>>();

    let r#override = matches.get_flag("override");

    let urls = picup(
        &api_url,
        &paths,
        &UploadImgParam::new(&token, 0, &category, r#override),
    )?;

    for url in urls {
        println!("{}", url);
    }

    Ok(())
}
