package dev.gpui.mobile;

import android.app.Activity;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.net.Uri;
import android.os.Bundle;

import java.util.ArrayList;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.atomic.AtomicReference;

/**
 * Transparent helper Activity that handles startActivityForResult calls.
 *
 * <p>NativeActivity cannot easily receive activity results, so this lightweight
 * transparent Activity is used as a proxy. It launches the requested intent,
 * captures the result, stores it in a static field, and finishes itself.</p>
 *
 * <p>Handles process death: when Android kills the process while the system
 * picker is in the foreground, this Activity is recreated from savedInstanceState.
 * The picker result is saved to SharedPreferences so it can be retrieved after
 * the app fully restarts.</p>
 *
 * <p>Called from Rust via JNI through GpuiFilePicker / GpuiImagePicker.</p>
 */
public class GpuiPickerActivity extends Activity {

    private static final int REQUEST_CODE = 9001;
    private static final String KEY_WAITING = "gpui_waiting_for_result";
    static final String PREFS_NAME = "gpui_picker_prefs";
    static final String PREF_PENDING_RESULT = "pending_result";
    static final String PREF_HAS_PENDING = "has_pending_result";

    /** Latch that the calling thread waits on. */
    static CountDownLatch sLatch;
    /** Result URIs from the picker. Null means cancelled. */
    static AtomicReference<ArrayList<String>> sResult = new AtomicReference<>(null);
    /** The intent to launch. Set before starting this Activity. */
    static Intent sPendingIntent;

    /** Whether we are waiting for an onActivityResult callback. */
    private boolean mWaitingForResult = false;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);

        if (savedInstanceState != null && savedInstanceState.getBoolean(KEY_WAITING, false)) {
            // Process was killed while the system picker was showing.
            // The system will recreate us and deliver the picker result via
            // onActivityResult. Just wait for it — don't try to launch again.
            android.util.Log.i("GpuiPicker", "Recreated after process death, waiting for result");
            mWaitingForResult = true;
            return;
        }

        if (sPendingIntent != null) {
            try {
                mWaitingForResult = true;
                startActivityForResult(sPendingIntent, REQUEST_CODE);
            } catch (Exception e) {
                android.util.Log.e("GpuiPicker", "Failed to start picker intent", e);
                mWaitingForResult = false;
                deliverResult(null);
                finish();
            }
        } else {
            deliverResult(null);
            finish();
        }
    }

    @Override
    protected void onSaveInstanceState(Bundle outState) {
        super.onSaveInstanceState(outState);
        outState.putBoolean(KEY_WAITING, mWaitingForResult);
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        mWaitingForResult = false;

        ArrayList<String> uris = null;
        if (requestCode == REQUEST_CODE && resultCode == RESULT_OK && data != null) {
            uris = extractUris(data);
        }

        deliverResult(uris);
        finish();
    }

    @Override
    public void onBackPressed() {
        mWaitingForResult = false;
        deliverResult(null);
        super.onBackPressed();
    }

    /**
     * Deliver the result to the waiting Rust thread, or save to SharedPreferences
     * if the process was recreated (sLatch is null).
     */
    private void deliverResult(ArrayList<String> uris) {
        if (sLatch != null) {
            // Normal path: Rust thread is waiting on the latch.
            sResult.set(uris);
            sLatch.countDown();
        } else {
            // Process death recovery: save to SharedPreferences for later retrieval.
            android.util.Log.i("GpuiPicker", "No latch (process death recovery), saving to prefs");
            savePendingResult(uris);
        }
    }

    /**
     * Extract URIs from the picker result intent.
     */
    private static ArrayList<String> extractUris(Intent data) {
        ArrayList<String> uris = new ArrayList<>();
        if (data.getClipData() != null) {
            int count = data.getClipData().getItemCount();
            for (int i = 0; i < count; i++) {
                Uri uri = data.getClipData().getItemAt(i).getUri();
                if (uri != null) {
                    uris.add(uri.toString());
                }
            }
        } else if (data.getData() != null) {
            uris.add(data.getData().toString());
        }
        return uris;
    }

    /**
     * Save picker result to SharedPreferences so it survives process death.
     */
    private void savePendingResult(ArrayList<String> uris) {
        SharedPreferences prefs = getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE);
        SharedPreferences.Editor editor = prefs.edit();
        if (uris != null && !uris.isEmpty()) {
            // Join with \n separator — URIs don't contain newlines.
            StringBuilder sb = new StringBuilder();
            for (int i = 0; i < uris.size(); i++) {
                if (i > 0) sb.append('\n');
                sb.append(uris.get(i));
            }
            editor.putBoolean(PREF_HAS_PENDING, true);
            editor.putString(PREF_PENDING_RESULT, sb.toString());
        } else {
            editor.putBoolean(PREF_HAS_PENDING, false);
            editor.remove(PREF_PENDING_RESULT);
        }
        editor.apply();
    }

    // ── Static helpers for Rust JNI access ──────────────────────────────

    /**
     * Check if there is a pending picker result from a previous process death.
     * Called from Rust via JNI.
     *
     * @param activity The current Activity context.
     * @return Array of URI strings, or null if no pending result.
     */
    public static String[] getPendingResult(Activity activity) {
        SharedPreferences prefs = activity.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE);
        if (!prefs.getBoolean(PREF_HAS_PENDING, false)) {
            return null;
        }
        String result = prefs.getString(PREF_PENDING_RESULT, null);
        // Clear the pending result.
        prefs.edit()
            .putBoolean(PREF_HAS_PENDING, false)
            .remove(PREF_PENDING_RESULT)
            .apply();

        if (result == null || result.isEmpty()) {
            return null;
        }
        return result.split("\n");
    }

    /**
     * Clear any pending picker result.
     * Called from Rust via JNI.
     */
    public static void clearPendingResult(Activity activity) {
        activity.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
            .edit()
            .putBoolean(PREF_HAS_PENDING, false)
            .remove(PREF_PENDING_RESULT)
            .apply();
    }
}
