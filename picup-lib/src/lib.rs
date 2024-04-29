use std::{
    env::temp_dir,
    fs::{remove_file, File},
    io::Write,
    path::PathBuf,
};

use reqwest::blocking::{multipart::Form, Client};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type Error = Box<dyn std::error::Error>;
pub type Result<T> = std::result::Result<T, Error>;

pub const API_BASE_URL: &str = "/picup";

#[macro_export]
macro_rules! api {
    ( $s: expr ) => {
        format!("{}{}", API_BASE_URL, $s).as_str()
    };
}

#[derive(PartialEq)]
pub struct ResponseCode(u16);

impl ResponseCode {
    pub fn to_u16(&self) -> u16 {
        self.0
    }
}

macro_rules! response_codes {
    (
        $(
            ($num:expr, $r#const:ident);
        )+
    ) => {
        impl ResponseCode {
        $(
            pub const $r#const: ResponseCode = ResponseCode($num);
        )+
        }
    }
}

response_codes! {
    (0, OK);
    (998, NOT_IMPLEMENTED);
    (999, INTERNAL_ERROR);
    (1001, INVALID_TOKEN);
    (1002, BAD_FILE_NAME);
    (1003, NOT_A_IMAGE);
    (1004, FILE_EXISTED);
    (1005, BAD_FILE);
    (1006, INVALID_CATEGORY);
}

fn serde_default_false() -> bool {
    false
}

fn serde_default_zero_u8() -> u8 {
    0
}

fn serde_default_empty_string() -> String {
    "".to_string()
}

// serde bug: https://github.com/serde-rs/serde/issues/1030
#[derive(Serialize, Deserialize)]
pub struct UploadImgParam {
    #[serde(default = "serde_default_false")]
    r#override: bool,

    #[serde(default = "serde_default_zero_u8")]
    compress: u8,

    #[serde(default = "serde_default_empty_string")]
    category: String,

    #[serde(default = "serde_default_empty_string")]
    access_token: String,
}

impl UploadImgParam {
    pub fn new(access_token: &str, compress: u8, category: &str, r#override: bool) -> Self {
        UploadImgParam {
            access_token: access_token.to_string(),
            compress,
            category: category.to_string(),
            r#override,
        }
    }

    pub fn r#override(&self) -> bool {
        self.r#override
    }

    pub fn compress(&self) -> u8 {
        self.compress
    }

    pub fn category(&self) -> &String {
        &self.category
    }

    pub fn access_token(&self) -> &String {
        &self.access_token
    }
}

#[derive(Serialize, Deserialize)]
pub struct GetImgParam {
    #[serde(default = "serde_default_zero_u8")]
    compress: u8,
}

impl GetImgParam {
    pub fn compress(&self) -> u8 {
        self.compress
    }
}

#[derive(Serialize, Deserialize)]
pub struct RestResponse<TData> {
    code: u16,
    msg: String,
    data: Option<TData>,
}

impl<TData> RestResponse<TData> {
    pub fn new(status: ResponseCode, msg: &str, data: TData) -> Self {
        Self {
            code: status.to_u16(),
            msg: msg.to_string(),
            data: Some(data),
        }
    }

    pub fn new_no_data(status: ResponseCode, msg: &str) -> Self {
        Self {
            code: status.to_u16(),
            msg: msg.to_string(),
            data: None,
        }
    }

    pub fn code(&self) -> ResponseCode {
        ResponseCode(self.code)
    }

    pub fn msg(&self) -> &String {
        &self.msg
    }

    pub fn data(&self) -> Option<&TData> {
        self.data.as_ref()
    }
}

pub fn picup<TPath>(
    base_url: &str,
    file_paths: &[TPath],
    param: &UploadImgParam,
) -> Result<Vec<String>>
where
    TPath: AsRef<std::path::Path>,
{
    let client = Client::new();

    let mut form = Form::new();

    let mut temp_files = vec![];

    for path in file_paths {
        if path.as_ref().starts_with("http") {
            // do nothing if it's actually a local file
            form = form.file("file", path)?;

            continue;
        }

        // download it before we add it
        let res = client
            .get(path.as_ref().to_str().unwrap())
            .send()?
            .bytes()?;

        let temp_file_path = [temp_dir(), Uuid::new_v4().to_string().into()]
            .iter()
            .collect::<PathBuf>();

        let mut file = File::create(&temp_file_path)?;

        file.write_all(&res)?;

        form = form.file("file", &temp_file_path)?;

        temp_files.push(temp_file_path);
    }

    let mut res = client
        .post(format!("{}{}", base_url, api!("/upload")))
        .query(&[
            ("access_token", param.access_token()),
            ("compress", &param.compress().to_string()),
            ("category", param.category()),
            ("override", &param.r#override().to_string()),
        ])
        .multipart(form)
        .send()?;

    let mut body_buf = vec![];
    res.copy_to(&mut body_buf)?;

    let json_str = String::from_utf8(body_buf)?;

    let res = match serde_json::from_str::<RestResponse<Vec<String>>>(&json_str) {
        Ok(parsed) => parsed,
        Err(e) => {
            eprintln!("json parse fail, should be an error: {}", e);

            return Err(Error::from(json_str));
        }
    };

    for file in temp_files {
        let _ = remove_file(file);
    }

    if res.code() != ResponseCode::OK {
        return Err(Error::from(res.msg().as_str()));
    }

    Ok(res.data().unwrap().to_vec())
}

#[test]
fn test() -> std::result::Result<(), Box<dyn std::error::Error>> {
    picup(
        "https://skopzz.com",
        &["D:/Download/demo.gif"],
        &UploadImgParam::new("baka", 0, "pic", false),
    )?;

    Ok(())
}
