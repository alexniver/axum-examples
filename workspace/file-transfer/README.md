# A File Share Server

## Use websocket to upload file, use stream download file

## Data Struct

``` doc
TotalLen(u32)Method(u8) DataLen(u32)Data([u8]) DataLen(u32)Data([u8])
```

| Method(u8) | Method Description   | Client Send Data Struct        | Client Recv Data Struct                   |
|------------|:--------------------:|--------------------------------|------------------------------------|
| 1          | Query file list      | (u32)(u8)                      | (u32)(u8)(u32)([u8])(u32)([u8])    |
| 2          | Upload File          | (u32)(u8)(u32)([u8])(u32)([u8])| (u32)(u8)(u8)                      |
| 3          | Download File        | (u32)(u8)(u32)([u8])           | (u32)(u8)(u32)([u8])(u32)([u8])    |
