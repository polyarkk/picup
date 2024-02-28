### 客户端指令

```bash
Usage: picup-cli.exe [OPTIONS] --token <token> [images]...

Arguments:
  [images]...  图片文件位置

Options:
  -t, --token <token>     服务器上传 Token
  -u, --apiurl <api_url>  "/upload" 接口访问 URL | 默认: http://127.0.0.1:19190
```

### 服务端指令

```bash
Usage: picup-srv.exe [OPTIONS] --token <token> --dir <dir>

Options:
  -t, --token <token>  服务器上传 Token
  -d, --dir <dir>      存放图片的位置
  -p, --port <port>    监听端口 | 默认: 19190
  -u, --url <url>      上传成功后返回给用户的 URL 前缀 | 默认: http://127.0.0.1:19190
```

