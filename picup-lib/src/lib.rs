use reqwest::blocking::{multipart::Form, Client};
use serde::{Deserialize, Serialize};

pub type Error = Box<dyn std::error::Error>;
pub type Result<T> = std::result::Result<T, Error>;

#[macro_export]
macro_rules! api {
    ( $s: expr ) => {
        format!("/picup{}", $s).as_str()
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
            ($num:expr, $konst:ident);
        )+
    ) => {
        impl ResponseCode {
        $(
            pub const $konst: ResponseCode = ResponseCode($num);
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
    base_url: &str, file_paths: &[TPath], param: &UploadImgParam
) -> Result<Vec<String>>
where
    TPath: AsRef<std::path::Path>,
{
    let client = Client::new();

    let mut form = Form::new();

    for path in file_paths {
        form = form.file("file", path)?;
    }

    let res = client
        .post(format!("{}{}", base_url, api!("/upload")))
        .query(&[
            ("access_token", param.access_token()),
            ("compress", &param.compress().to_string()),
            ("category", param.category()), 
            ("override", &param.r#override().to_string())]
        )
        .multipart(form)
        .send()?
        .json::<RestResponse<Vec<String>>>()?;

    if res.code() != ResponseCode::OK {
        return Err(Error::from(res.msg().as_str()));
    }

    Ok(res.data().unwrap().to_vec())
}
