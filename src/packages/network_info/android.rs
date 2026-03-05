use super::NetworkInfo;
use crate::android::jni::{self as jni_helpers, get_string};
use jni::objects::JValue;

pub fn get_network_info() -> Result<NetworkInfo, String> {
    jni_helpers::with_env(|env| {
        let activity = jni_helpers::activity()?;
        let mut info = NetworkInfo::default();

        // context.getSystemService("wifi") → WifiManager
        let service_name = env.new_string("wifi").map_err(|e| e.to_string())?;
        let wifi_mgr = match env
            .call_method(
                &activity,
                "getSystemService",
                "(Ljava/lang/String;)Ljava/lang/Object;",
                &[JValue::Object(&service_name)],
            )
            .and_then(|v| v.l())
        {
            Ok(o) if !o.is_null() => o,
            _ => {
                let _ = env.exception_clear();
                return Ok(info);
            }
        };

        // wifiManager.getConnectionInfo() → WifiInfo
        let wifi_info = match env
            .call_method(
                &wifi_mgr,
                "getConnectionInfo",
                "()Landroid/net/wifi/WifiInfo;",
                &[],
            )
            .and_then(|v| v.l())
        {
            Ok(o) if !o.is_null() => o,
            _ => {
                let _ = env.exception_clear();
                return Ok(info);
            }
        };

        // SSID
        if let Ok(ssid_obj) = env
            .call_method(&wifi_info, "getSSID", "()Ljava/lang/String;", &[])
            .and_then(|v| v.l())
        {
            let ssid = get_string(env, &ssid_obj);
            let ssid = ssid.trim_matches('"').to_string();
            if !ssid.is_empty() && ssid != "<unknown ssid>" {
                info.wifi_name = Some(ssid);
            }
        }

        // BSSID
        if let Ok(bssid_obj) = env
            .call_method(&wifi_info, "getBSSID", "()Ljava/lang/String;", &[])
            .and_then(|v| v.l())
        {
            let bssid = get_string(env, &bssid_obj);
            if !bssid.is_empty() && bssid != "02:00:00:00:00:00" {
                info.wifi_bssid = Some(bssid);
            }
        }

        // IP Address
        if let Ok(ip) = env
            .call_method(&wifi_info, "getIpAddress", "()I", &[])
            .and_then(|v| v.i())
        {
            if ip != 0 {
                info.wifi_ip = Some(format!(
                    "{}.{}.{}.{}",
                    ip & 0xFF,
                    (ip >> 8) & 0xFF,
                    (ip >> 16) & 0xFF,
                    (ip >> 24) & 0xFF,
                ));
            }
        }

        Ok(info)
    })
}
