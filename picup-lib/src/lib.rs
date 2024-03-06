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

fn serde_default_empty_string() -> String {
    "".to_string()
}

// serde bug: https://github.com/serde-rs/serde/issues/1030
#[derive(Serialize, Deserialize)]
pub struct UploadImgParam {
    #[serde(default = "serde_default_false")]
    r#override: bool,

    #[serde(default = "serde_default_empty_string")]
    category: String,

    access_token: String,
}

impl UploadImgParam {
    pub fn new(access_token: &str, category: &str, r#override: bool) -> Self {
        UploadImgParam {
            access_token: access_token.to_string(),
            category: category.to_string(),
            r#override,
        }
    }

    pub fn r#override(&self) -> bool {
        self.r#override
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

/**
   Uploads images to the PicUp server using `/upload` API.

   `base_url`: Base url of the API. On calling, the url will be: `{baseurl}/picup/upload`.

   `token`: Access token for the API.

   `file_paths`: Paths of image files.

   If OK, the function will return a vector of urls for images uploaded, or error messages otherwise.

   ```rust
    use picup_lib::picup;

    let url = "http://127.0.0.1:19190";
    let token = "baka";
    let file_paths = vec!["/path/to/img1", "/path/to/img2"];

    let result = picup(url, token, &file_paths);
   ```
*/
pub fn picup<TPath>(
    base_url: &str, token: &str, category: &str, file_paths: &[TPath], r#override: bool
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
        .query(&[("access_token", token), ("category", category), ("override", &r#override.to_string())])
        .multipart(form)
        .send()?
        .json::<RestResponse<Vec<String>>>()?;

    if res.code() != ResponseCode::OK {
        return Err(Error::from(res.msg().as_str()));
    }

    Ok(res.data().unwrap().to_vec())
}
