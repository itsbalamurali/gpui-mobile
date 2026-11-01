package dev.gpui.mobile.example;

import android.app.Activity;
import android.app.Instrumentation;
import android.content.Intent;
import android.os.Bundle;
import android.os.SystemClock;

import java.lang.reflect.Method;

/** Exercises the native Activity lifecycle without replacing the app's stored data. */
public final class ActivityLifecycleTest extends Instrumentation {
    private static final String ACTIVITY = "dev.gpui.mobile.GpuiActivity";
    private static final long TIMEOUT_MS = 30000;
    private String scenario;

    @Override
    public void onCreate(Bundle arguments) {
        super.onCreate(arguments);
        scenario = arguments.getString("scenario", "recreate");
        start();
    }

    @Override
    public void onStart() {
        Bundle result = new Bundle();
        try {
            Intent launch = new Intent(Intent.ACTION_MAIN)
                    .setClassName(getTargetContext().getPackageName(), ACTIVITY)
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            Activity activity = startActivitySync(launch);
            awaitReady(activity);
            if ("recreate".equals(scenario)) {
                recreate(activity);
            } else if ("background".equals(scenario)) {
                backgroundResume(activity, launch);
            } else {
                throw new IllegalArgumentException("Unknown scenario: " + scenario);
            }
            result.putString("stream", "PASS: " + scenario + "\n");
            finish(Activity.RESULT_OK, result);
        } catch (Throwable failure) {
            result.putString("stream", "FAIL: " + android.util.Log.getStackTraceString(failure));
            finish(Activity.RESULT_CANCELED, result);
        }
    }

    private void recreate(Activity activity) throws Exception {
        int pid = android.os.Process.myPid();
        // Each new Activity must have a usable native window in the same process.
        for (int generation = 1; generation <= 3; generation++) {
            ActivityMonitor monitor = addMonitor(ACTIVITY, null, false);
            Activity previous = activity;
            try {
                runOnMainSync(previous::recreate);
                activity = monitor.waitForActivityWithTimeout(TIMEOUT_MS);
                if (activity == null || activity == previous) {
                    throw new AssertionError("Activity was not recreated");
                }
                awaitReady(activity);
                if (android.os.Process.myPid() != pid) {
                    throw new AssertionError("Process restarted during recreation");
                }
            } finally {
                removeMonitor(monitor);
            }
        }
        Activity last = activity;
        runOnMainSync(last::finish);
    }

    private void backgroundResume(Activity activity, Intent launch) throws Exception {
        Intent home = new Intent(Intent.ACTION_MAIN)
                .addCategory(Intent.CATEGORY_HOME)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
        getTargetContext().startActivity(home);
        long deadline = SystemClock.uptimeMillis() + TIMEOUT_MS;
        boolean[] focused = new boolean[1];
        do {
            runOnMainSync(() -> focused[0] = activity.hasWindowFocus());
            if (!focused[0]) {
                break;
            }
            SystemClock.sleep(50);
        } while (SystemClock.uptimeMillis() < deadline);
        if (focused[0]) {
            throw new AssertionError("Activity did not enter background");
        }
        SystemClock.sleep(1500);
        getTargetContext().startActivity(launch);
        awaitReady(activity);
        runOnMainSync(activity::finish);
    }

    private void awaitReady(Activity activity) throws Exception {
        Method ready = activity.getClass().getDeclaredMethod("isNativeReady");
        ready.setAccessible(true);
        long deadline = SystemClock.uptimeMillis() + TIMEOUT_MS;
        boolean[] usable = new boolean[1];
        do {
            runOnMainSync(() -> {
                try {
                    usable[0] = !activity.isDestroyed() && activity.hasWindowFocus()
                            && (Boolean) ready.invoke(activity);
                } catch (ReflectiveOperationException error) {
                    throw new AssertionError(error);
                }
            });
            if (usable[0]) {
                // Give the native loop time to surface a crash after initialization.
                SystemClock.sleep(1000);
                return;
            }
            SystemClock.sleep(50);
        } while (SystemClock.uptimeMillis() < deadline);
        throw new AssertionError("Native window did not become ready");
    }
}
