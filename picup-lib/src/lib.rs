use axum::http::StatusCode;
use reqwest::blocking::{multipart::Form, Client};
use serde::{Deserialize, Serialize};

pub type Error = Box<dyn std::error::Error>;
pub type Result<T> = std::result::Result<T, Error>;

#[macro_export]
macro_rules! api {
    ($s:expr) => {
        format!("/picup{}", $s).as_str()
    };
}

#[derive(PartialEq)]
pub struct ResponseCode(u16);

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
    (999, INTERNAL_ERROR);
    (1001, INVALID_TOKEN);
    (1002, BAD_FILE_NAME);
    (1003, NOT_A_IMAGE);
    (1004, FILE_EXISTED);
    (1005, BAD_FILE);
}

#[derive(Serialize, Deserialize)]
pub struct UploadImgParam {
    r#override: Option<bool>,
    access_token: String,
}

impl UploadImgParam {
    pub fn new(access_token: &str) -> Self {
        UploadImgParam {
            access_token: access_token.to_string(),
            r#override: None,
        }
    }

    pub fn new_override(access_token: &str) -> Self {
        UploadImgParam {
            access_token: access_token.to_string(),
            r#override: Some(true),
        }
    }

    pub fn r#override(&self) -> bool {
        self.r#override.is_some() && self.r#override.unwrap()
    }

    pub fn access_token(&self) -> &str {
        &self.access_token
    }
}

#[derive(Serialize, Deserialize)]
pub struct RestResponse<TData> {
    status: u16,
    msg: String,
    data: Option<TData>,
}

impl<TData> RestResponse<TData> {
    pub fn ok(data: TData) -> Self {
        RestResponse {
            status: StatusCode::OK.as_u16(),
            msg: format!("ok"),
            data: Some(data),
        }
    }

    pub fn no(code: ResponseCode, msg: String) -> Self {
        RestResponse {
            status: code.0,
            msg,
            data: None,
        }
    }

    pub fn status(&self) -> ResponseCode {
        ResponseCode(self.status)
    }

    pub fn msg(&self) -> &String {
        &self.msg
    }

    pub fn data(&self) -> Option<&TData> {
        self.data.as_ref()
    }
}

/**
   Uploads images to the PicUp server using `/upload` API.

   `base_url`: Base url of the API. On calling, the url will be: `{baseurl}/picup/upload`.

   `token`: Access token for the API.

   `file_paths`: Paths of image files.

   If OK, the function will return a vector of urls for images uploaded, or error messages otherwise.

   ```rust
    let url = "http://127.0.0.1:19190";
    let token = "baka";
    let file_paths = vec!["/path/to/img1", "/path/to/img2"];

    let result = picup(url, token, &file_paths);
   ```
*/
pub fn picup<TPath>(base_url: &str, token: &str, file_paths: &[TPath]) -> Result<Vec<String>>
where
    TPath: AsRef<std::path::Path>,
{
    let client = Client::new();

    let mut form = Form::new();

    for path in file_paths {
        form = form.file("file", path).expect("invalid file path");
    }

    let res = client
        .post(format!("{}{}", base_url, api!("/upload")))
        .query(&[("access_token", token)])
        .multipart(form)
        .send()?
        .json::<RestResponse<Vec<String>>>()?;

    if res.status() != ResponseCode::OK {
        return Err(Error::from(res.msg().as_str()));
    }

    return Ok(res.data().unwrap().to_vec());
}
