use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer};
use green_copper_runtime::moduleloaders::{FileSystemModuleLoader, HttpModuleLoader};
use hirofa_utils::js_utils::adapters::proxies::JsProxy;
use hirofa_utils::js_utils::adapters::{JsRealmAdapter, JsValueAdapter};
use hirofa_utils::js_utils::facades::{JsRuntimeBuilder, JsRuntimeFacade};
use hirofa_utils::js_utils::{JsError, Script};
use lazy_static::lazy_static;
use log::LevelFilter;
use quickjs_runtime::builder::QuickJsRuntimeBuilder;
use quickjs_runtime::facades::QuickJsRuntimeFacade;
use typescript_utils::{TargetVersion, TypeScriptPreProcessor};

lazy_static! {
    static ref SCRIPT_RT: QuickJsRuntimeFacade = init_quickjs();
}

fn init_quickjs() -> QuickJsRuntimeFacade {
    let tspp = TypeScriptPreProcessor::new(TargetVersion::Es2020, false, false);
    let fsml = FileSystemModuleLoader::new("./modules");
    let html = HttpModuleLoader::new()
        .secure_only()
        .allow_domain("https://github.com");

    let mut builder = QuickJsRuntimeBuilder::new()
        .script_pre_processor(tspp)
        .js_script_module_loader(fsml)
        .js_script_module_loader(html);

    builder = green_copper_runtime::init_greco_rt(builder);
    let rt = builder.build();
    // to install out proxy we add a job to the RuntimeFacade
    // we won't use multiple realms so we pass None as realm_name, this will make the runtime use the main realm (or context)
    rt.js_loop_realm_sync(None, |_rt, realm| {
        init_proxy(realm)?;
        let res: Result<(), JsError> = Ok(());
        res
    })
    .ok()
    .expect("init proxy failed");
    return rt;
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

async fn do_dispatch() {
    // for every request we add a job to the script engine and await until it is done
    SCRIPT_RT
        .js_loop_realm(None, |_rt, realm| {
            // dispatch the request event to our proxy class
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

    SCRIPT_RT
        .js_eval_module(None, Script::new("file://main.ts", include_str!("main.ts")))
        .await
        .ok()
        .expect("main.ts failed");
    HttpServer::new(|| App::new().service(web::resource("/").to(index)))
        .bind(("0.0.0.0", 8070))?
        .run()
        .await
}
