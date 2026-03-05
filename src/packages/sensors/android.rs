use super::{BarometerData, SensorAvailability, SensorData};
use crate::android::jni as jni_helpers;
use jni::objects::JValue;

// Android Sensor.TYPE_* constants
const TYPE_ACCELEROMETER: i32 = 1;
const TYPE_GYROSCOPE: i32 = 4;
const TYPE_MAGNETIC_FIELD: i32 = 2;
const TYPE_PRESSURE: i32 = 6;

pub fn available_sensors() -> SensorAvailability {
    let mut env = match jni_helpers::obtain_env() {
        Ok(e) => e,
        Err(_) => return SensorAvailability::default(),
    };
    let activity = match jni_helpers::activity() {
        Ok(a) => a,
        Err(_) => return SensorAvailability::default(),
    };

    let sm = match get_sensor_manager(&mut env, &activity) {
        Some(sm) => sm,
        None => return SensorAvailability::default(),
    };

    SensorAvailability {
        accelerometer: has_sensor(&mut env, &sm, TYPE_ACCELEROMETER),
        gyroscope: has_sensor(&mut env, &sm, TYPE_GYROSCOPE),
        magnetometer: has_sensor(&mut env, &sm, TYPE_MAGNETIC_FIELD),
        barometer: has_sensor(&mut env, &sm, TYPE_PRESSURE),
    }
}

pub fn accelerometer() -> Option<SensorData> {
    // Single-shot sensor reads require registering a SensorEventListener via JNI,
    // which involves creating a Java callback object. This is complex with pure JNI.
    // For now, sensor data is available through the availability check.
    // A full implementation would use android.hardware.SensorManager with a listener
    // or the NDK ASensorManager API with an ALooper.
    None
}

pub fn gyroscope() -> Option<SensorData> {
    None
}

pub fn magnetometer() -> Option<SensorData> {
    None
}

pub fn barometer() -> Option<BarometerData> {
    None
}

fn get_sensor_manager<'local>(
    env: &mut jni::JNIEnv<'local>,
    activity: &jni::objects::JObject<'_>,
) -> Option<jni::objects::JObject<'local>> {
    let service_name = env.new_string("sensor").ok()?;
    let sm = env
        .call_method(
            activity,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[JValue::Object(&service_name)],
        )
        .and_then(|v| v.l())
        .ok()?;
    if sm.is_null() {
        let _ = env.exception_clear();
        None
    } else {
        Some(sm)
    }
}

fn has_sensor(
    env: &mut jni::JNIEnv<'_>,
    sm: &jni::objects::JObject<'_>,
    sensor_type: i32,
) -> bool {
    match env
        .call_method(
            sm,
            "getDefaultSensor",
            "(I)Landroid/hardware/Sensor;",
            &[JValue::Int(sensor_type)],
        )
        .and_then(|v| v.l())
    {
        Ok(sensor) => !sensor.is_null(),
        Err(_) => {
            let _ = env.exception_clear();
            false
        }
    }
}
