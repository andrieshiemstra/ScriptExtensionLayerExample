# ScriptExtensionLayerExample
Example project on embedding a JavaScript engine in a rust project.

What we want is to embed a script engine which can receive events which occur in our rust app and add functionality by using javascript or typescript.

The projects we'll be using to achieve this are the following
* [quickjs_runtime](https://github.com/HiRoFa/quickjs_es_runtime) the quickjs wrapper library
* [hirofa_utils](https://github.com/HiRoFa/utils) ScriptEngine abstraction layer and utilities
* [GreenCopperRuntime](https://github.com/HiRoFa/GreenCopperRuntime) adds more features to JavaScript engine like fetch and module loaders
* [typescript_utils](https://github.com/HiRoFa/typescript_utils) Typescript transpiler (uses SWC)

You can read more about the quickjs_runtime project inner workings here https://hirofa.github.io/quickjs_es_runtime/quickjs_runtime/index.html 

## The sample application

Our sample application will be a simple actix based webapp with a simple request handler

Async support will be provided by using tokio

[`Cargo.toml`](Cargo.toml)
```toml
[package]
name = "my_app"
version = "0.1.0"
authors = ["Andries Hiemstra <andries@hiemstra-software.nl>"]
edition = "2018"

[dependencies]
tokio = "1"
actix-web = "4.0.0-rc.3"
log = "0.4"

```

[`main.rs`](src/main.rs)
```rust
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer};

async fn index(_req: HttpRequest) -> HttpResponse {
    HttpResponse::Ok().body("hello there")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| App::new().service(web::resource("/").to(index)))
        .bind(("0.0.0.0", 8070))?
        .run()
        .await
}

```

## Adding the script engine and running our main.js file

We add the following dependencies to our [`Cargo.toml`](Cargo.toml)
```toml
[dependencies]
...
quickjs_runtime = "0.7.1"
hirofa_utils = "0.4"
lazy_static = "1.4.0"
...
```

And we init a runtime as a lazy static in our [`main.rs`](src/main.rs)
```rust
...
use lazy_static::lazy_static;
use quickjs_runtime::builder::QuickJsRuntimeBuilder;
use quickjs_runtime::facades::QuickJsRuntimeFacade;

lazy_static! {
    static ref SCRIPT_RT: QuickJsRuntimeFacade = init_quickjs();
}

fn init_quickjs() -> QuickJsRuntimeFacade {
    let rt = QuickJsRuntimeBuilder::new().build();
    // init code here
    return rt;
}
...
```

## Creating the MyApplication proxy object

In order to give our script something to communicate with we'll define a Proxy class 

This proxy class will be used to dispatch events and add functions for our script file to invoke.

A bit of terminology: when working with the thread-safe objects you work with facades (JsRuntimeFacade, JsValueFacade) when working directly with Script objects ion the worker thread of the script engine you'll be working with Adapters(JsRuntimeAdapter, JsRealmAdapter, JsValueAdapter). These adapter cannot be moved out of the worker thread. To pass values from and to teh worker thread you need to convert them to JsValueFacades using `JsRealmAdapter.to_js_value_facade()` and `JsRealmAdapter.from_js_value_facade()`  

in [`main.rs`](src/main.rs) we add the following:
```rust
...
use hirofa_utils::js_utils::adapters::proxies::JsProxy;
use hirofa_utils::js_utils::adapters::{JsRealmAdapter, JsValueAdapter};
use hirofa_utils::js_utils::facades::JsRuntimeFacade;
use hirofa_utils::js_utils::JsError;
...
fn init_quickjs() -> QuickJsRuntimeFacade {
    ...
    // to install out proxy we add a job to the RuntimeFacade
    // we won't use multiple realms so we pass None as realm_name, this will make the runtime use the main realm (or context)
    rt.js_loop_realm_sync(None, |_rt, realm| {
        init_proxy(realm)?;
        let res: Result<(), JsError> = Ok(());
        res
    })
        .ok()
        .expect("init proxy failed");
    ...
}


fn init_proxy<R: JsRealmAdapter>(realm: &R) -> Result<(), JsError> {
    let proxy = JsProxy::new(&["com", "mycompany"], "MyApp")
        // out proxy wil have a single static method printSomething
        .add_static_method("printSomething", |_rt, realm: &R, args| {
            // if first arg is a string, log that string
            if args[0].js_is_string() {
                println!("script printed: {}", args[0].js_to_str()?)
            }
            // return undefined
            realm.js_undefined_create()
        })
        // setting the static_event_target to true means we can dispatch events and and add listeners from script
        // by calling com.mycompany.MyApp.addEventListener()
        .set_static_event_target(true);
    // we set add_global_var to true so there will be a com.mycompany.MyApp usable in script
    realm.js_proxy_install(proxy, true)?;
    Ok(())
}
...
```

## Dispatching events and responding to them

Now in order to actually get something to work we need to dispatch events to our script engine and respond to them from javascript.

in in [`main.rs`](src/main.rs) we add a function to dispatch an event and call it for every request

```rust
...
async fn do_dispatch() {
    // for every request we add a job to the script engine and await until it is done
    SCRIPT_RT
        .js_loop_realm(None, |_rt, realm| {
            // dispatch the request event to our proxy class, our event obj wil ne null for now
            let event_obj = realm
                .js_null_create()
                .ok()
                .expect("could not create event obj");
            match realm.js_proxy_dispatch_static_event(
                &["com", "mycompany"],
                "MyApp",
                "request",
                &event_obj,
            ) {
                Ok(_vetoed) => {
                    //
                }
                Err(err) => {
                    log::error!("could not dispatch event: {}", err);
                }
            }
        })
        .await;
}

async fn index(_req: HttpRequest) -> HttpResponse {
    do_dispatch().await;
    HttpResponse::Ok().body("hello there")
}
...
```

Now in order to respond to that event we add the following to our [`main.js`](src/main.js) file:
```javascript
com.mycompany.MyApp.addEventListener("request", (evt) => {
    com.mycompany.MyApp.printSomething("Just letting you know javascript received your event loud and clear!");
});
```

and we run `main.js` when starting actix in `main.rs`

```rust
...
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    SCRIPT_RT
        .js_eval(None, Script::new("file://main.js", include_str!("main.js")))
        .await
        .ok()
        .expect("main.js failed");
...
```

Now when we run our app and point our browser to `http://localhost:8070/` the output in our console will be as follows:

```
    Finished dev [unoptimized + debuginfo] target(s) in 0.08s
     Running `target/debug/my_app`
script printed: Just letting you know javascript received your event loud and clear!
```

**Eureka!**

## Using GreenCopperRuntime to add features to JavaScript (fetch/console)

To get some more possibilities in our newly created script-enabled application we're going to add support for fetch and console.log

In order to keep quickjs_runtime as clean as possible I decided to add these things to a separate project called [GreenCopper](https://github.com/HiRoFa/GreenCopperRuntime) you know... because copper connects everything and when it **rust**s it turns green :)  

Lets add GreCo to our [`Cargo.toml`](Cargo.toml)

The "com" feature includes fetch, the "features" feature includes the console, the "db" features includes mysql query utils

```toml
[dependencies]
...
green_copper_runtime =  { git = 'https://github.com/HiRoFa/GreenCopperRuntime', branch="main", features = ["com", "features", "db"], default-features=false}
...
```

And then in [`main.rs`](src/main.rs) we add the GreCo features to our builder.

```rust
fn init_quickjs() -> QuickJsRuntimeFacade {
    let mut builder = QuickJsRuntimeBuilder::new();
    builder = green_copper_runtime::init_greco_rt(builder);
    let rt = builder.build();
    ...
```

### fetch

The fetch api should work the same as fetch as defined at MDN, it is currently a pretty minimal implementation but simple calls should work.

### console

The console enables use of things like `console.log` and `console.debug`

it uses the `log` crate so you should be able to log things to log files by using .e.g `simple-logging` in [`main.rs`](src/main.rs)  

```rust
...
#[actix_web::main]
async fn main() -> std::io::Result<()> {

    #[cfg(not(debug_assertions))]
    {
        simple_logging::log_to_file("myapp.log", LevelFilter::Info)?;
    }
    #[cfg(debug_assertions)]
    {
        simple_logging::log_to_file("myapp.log", LevelFilter::Trace)?;
    }
```

and in [`main.js`](src/main.js)
```javascript
...
console.log("logging from javascript");
...
```

will result in this line in `myapp.log`
```
[00:00:02.263] (7fef18f37640) INFO   JS_REALM:[__main__]: logging from javascript
```

## Adding typescript support

quickjs_runtime has support for script preprocessors. in the code above you unknowingly added a cpp style preprocessor enabling you to do things like this
```javascript
...
#IFDEF DEBUG
console.trace("logging some mighty large strings" + " which you don't want to construct in release mode");
#ENDIF
...
```

And that same principle allows us to add a typescript transpiler, this functionality is still in beta and therefore in a repo of its own at https://github.com/HiRoFa/typescript_utils

in order to enable this we need to add a dependency to our Cargo.toml
```toml
[dependencies]
...
typescript_utils = {git="https://github.com/HiRoFa/typescript_utils"}
...
```

and add it to our builder in [`main.rs`](src/main.rs)

```rust
...
use typescript_utils::{TargetVersion, TypeScriptPreProcessor};
...
fn init_quickjs() -> QuickJsRuntimeFacade {
    let tspp = TypeScriptPreProcessor::new(TargetVersion::Es2020, false, false);
    let mut builder = QuickJsRuntimeBuilder::new().script_pre_processor(tspp);
```

The nice thing about using typescript is that we can define in script what our proxy looks like making it easier to develop scripts.

Now we can replace our [`main.js`](src/main.rs) with [`main.ts`](src/main.ts)

don't forget to alter the init code which runs `main.js` in `main.rs`
```rust
...
SCRIPT_RT
        .js_eval(None, Script::new("file://main.ts", include_str!("main.ts")))
        .await
        .ok()
        .expect("main.ts failed");
...
```

our main.ts looks like this

```typescript
type MyApp = EventTarget & {
    printSomething: (thing: string) => void
};

const myApp: MyApp = com.mycompany.MyApp;

com.mycompany.MyApp.addEventListener("request", (evt) => {
    myApp.printSomething("Just letting you know javascript received your event loud and clear!");
    console.log("logging from javascript");
});
```

## Adding module loaders to load modules from filesystem and http

Last but not least we're going to enable your scripters to include modules from the filesystem as well as online.

To achieve this we need to add two ModuleLoaders to our builder

```rust
...
use green_copper_runtime::moduleloaders::{FileSystemModuleLoader, HttpModuleLoader};
...
fn init_quickjs() -> QuickJsRuntimeFacade {
    let tspp = TypeScriptPreProcessor::new(TargetVersion::Es2020, false, false);
    // a filesystem module loader which loads modules form ./modules
    let fsml = FileSystemModuleLoader::new("./modules");
    // a http module loader whioch is allowed to load modules via https only from github.com
    let html = HttpModuleLoader::new()
        .secure_only()
        .allow_domain("https://github.com");

    let mut builder = QuickJsRuntimeBuilder::new()
        .script_pre_processor(tspp)
        .js_script_module_loader(fsml)
        .js_script_module_loader(html);
    
    builder = green_copper_runtime::init_greco_rt(builder);
    let rt = builder.build();
...
```

now in [`main.ts`](src/main.ts) we can load modules like this

```typescript
    import {calc} from 'ModuleA.ts';
    console.log("7*8=%s", calc(7, 8));
    ...
```

This does however mean we need to run main.ts as a module and not just as a script file so in [`main.rs`](src/main.rs) we alter our init code

```rust
...
SCRIPT_RT
        .js_eval_module(None, Script::new("file://main.ts", include_str!("main.ts")))
        .await
        .ok()
        .expect("main.ts failed");
...
```

## Final thoughts

I hope this gives you an idea of how to implement a javascript or typescript engine in your rust project.


