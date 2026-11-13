//! UTF-8 and standard Base64 helpers used by the public SDK's chunked storage.
use base64::{engine::general_purpose::STANDARD, Engine};
use rquickjs::{Ctx, Error, Function, TypedArray};

fn encode_utf8<'js>(ctx: Ctx<'js>, value: String) -> rquickjs::Result<TypedArray<'js, u8>> {
    TypedArray::new(ctx, value.into_bytes())
}

fn decode_utf8(bytes: TypedArray<'_, u8>, fatal: bool) -> rquickjs::Result<String> {
    let bytes = bytes
        .as_bytes()
        .ok_or_else(|| Error::new_from_js_message("buffer", "UTF-8", "Detached byte buffer"))?;
    if fatal {
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|error| Error::new_from_js_message("bytes", "UTF-8", error.to_string()))
    } else {
        Ok(String::from_utf8_lossy(bytes).into_owned())
    }
}

pub fn install(ctx: &Ctx<'_>) -> rquickjs::Result<()> {
    ctx.globals()
        .set("__nextUtf8", Function::new(ctx.clone(), encode_utf8)?)?;
    ctx.globals()
        .set("__nextDecodeUtf8", Function::new(ctx.clone(), decode_utf8)?)?;
    ctx.globals().set(
        "__nextEncodeBase64",
        Function::new(ctx.clone(), |value: String| {
            let bytes = value
                .chars()
                .map(|character| u8::try_from(u32::from(character)))
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|_| {
                    Error::new_from_js_message("string", "Latin-1", "Invalid character")
                })?;
            Ok::<_, Error>(STANDARD.encode(bytes))
        })?,
    )?;
    ctx.globals().set(
        "__nextDecodeBase64",
        Function::new(ctx.clone(), |value: String| {
            STANDARD
                .decode(value)
                .map(|bytes| bytes.into_iter().map(char::from).collect::<String>())
                .map_err(|error| Error::new_from_js_message("base64", "bytes", error.to_string()))
        })?,
    )?;
    ctx.eval::<(), _>(
        r#"
globalThis.TextEncoder=class TextEncoder {
  encode(value=''){return __nextUtf8(String(value));}
};
globalThis.TextDecoder=class TextDecoder {
  constructor(label='utf-8',options={}){
    if(!['utf-8','utf8'].includes(String(label).toLowerCase()))throw RangeError('Only UTF-8 is supported');
    this.fatal=Boolean(options.fatal);this.ignoreBOM=Boolean(options.ignoreBOM);
  }
  decode(value=new Uint8Array(),options={}){
    if(options.stream)throw TypeError('Streaming UTF-8 decoding is not supported');
    let bytes;
    if(value instanceof ArrayBuffer)bytes=new Uint8Array(value);
    else if(ArrayBuffer.isView(value))bytes=new Uint8Array(value.buffer,value.byteOffset,value.byteLength);
    else throw TypeError('Expected a byte buffer');
    let text=__nextDecodeUtf8(bytes,this.fatal);
    if(!this.ignoreBOM&&text.charCodeAt(0)===0xfeff)text=text.slice(1);
    return text;
  }
};
globalThis.btoa=value=>__nextEncodeBase64(String(value));
globalThis.atob=value=>__nextDecodeBase64(String(value));
"#,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rquickjs::{Context, Runtime};

    #[test]
    fn sdk_encoding_roundtrips_unicode_binary_and_rejects_invalid_input() {
        let runtime = Runtime::new().unwrap();
        runtime.set_memory_limit(64 * 1024 * 1024);
        runtime.set_max_stack_size(512 * 1024);
        let context = Context::full(&runtime).unwrap();
        context.with(|ctx| {
            install(&ctx).unwrap();
            assert!(ctx
                .eval::<bool, _>(
                    r#"
                const text='任务导出🌏\n';
                const bytes=new TextEncoder().encode(text);
                const encoded=btoa(String.fromCharCode(...bytes));
                const output=Uint8Array.from(atob(encoded),x=>x.charCodeAt(0));
                new TextDecoder('utf-8',{fatal:true}).decode(output)===text
            "#
                )
                .unwrap());
            for script in [
                "new TextDecoder('utf-8',{fatal:true}).decode(new Uint8Array([255]))",
                "atob('!invalid')",
                "btoa('中文')",
                "new TextDecoder('shift-jis')",
                "new TextDecoder().decode(new Uint8Array(),{stream:true})",
            ] {
                assert!(ctx.eval::<rquickjs::Value, _>(script).is_err(), "{script}");
                ctx.catch();
            }
            // Reproduce a multi-entry month under the real runner's memory limit.
            assert!(ctx
                .eval::<bool, _>(
                    r#"
                const large='任务中心导出：中文🌏\n'.repeat(54000);
                const encodedLarge=new TextEncoder().encode(large);
                new TextDecoder('utf-8',{fatal:true}).decode(encodedLarge)===large
            "#
                )
                .unwrap());
        });
    }
}
