use super::ConnectivityStatus;
use crate::android::jni_helpers;
use jni::objects::JValue;

pub fn check_connectivity() -> ConnectivityStatus {
    let mut env = match jni_helpers::obtain_env() {
        Ok(e) => e,
        Err(_) => return ConnectivityStatus::None,
    };
    let activity = match jni_helpers::activity() {
        Ok(a) => a,
        Err(_) => return ConnectivityStatus::None,
    };

    // context.getSystemService("connectivity") → ConnectivityManager
    let service_name = match env.new_string("connectivity") {
        Ok(s) => s,
        Err(_) => return ConnectivityStatus::None,
    };
    let cm = match env
        .call_method(
            &activity,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[JValue::Object(&service_name)],
        )
        .and_then(|v| v.l())
    {
        Ok(o) if !o.is_null() => o,
        _ => { let _ = env.exception_clear(); return ConnectivityStatus::None; }
    };

    // cm.getActiveNetworkInfo() → NetworkInfo
    let net_info = match env
        .call_method(&cm, "getActiveNetworkInfo", "()Landroid/net/NetworkInfo;", &[])
        .and_then(|v| v.l())
    {
        Ok(o) if !o.is_null() => o,
        _ => { let _ = env.exception_clear(); return ConnectivityStatus::None; }
    };

    // networkInfo.isConnected()
    let connected = match env
        .call_method(&net_info, "isConnected", "()Z", &[])
        .and_then(|v| v.z())
    {
        Ok(c) => c,
        Err(_) => { let _ = env.exception_clear(); return ConnectivityStatus::None; }
    };
    if !connected {
        return ConnectivityStatus::None;
    }

    // networkInfo.getType()
    match env
        .call_method(&net_info, "getType", "()I", &[])
        .and_then(|v| v.i())
    {
        Ok(1) => ConnectivityStatus::Wifi,     // TYPE_WIFI
        Ok(0) => ConnectivityStatus::Cellular,  // TYPE_MOBILE
        Ok(_) => ConnectivityStatus::Wifi,      // Ethernet etc. treated as Wifi
        Err(_) => { let _ = env.exception_clear(); ConnectivityStatus::None }
    }
}
