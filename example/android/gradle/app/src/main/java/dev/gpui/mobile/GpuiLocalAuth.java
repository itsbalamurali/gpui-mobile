package dev.gpui.mobile;

import android.app.Activity;
import android.content.Intent;
import android.os.Build;

import java.util.ArrayList;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.atomic.AtomicInteger;

/**
 * Biometric authentication helper for GPUI.
 *
 * <p>Uses BiometricManager (API 29+) for availability checks and launches
 * {@link GpuiAuthActivity} (a transparent FragmentActivity) to show the
 * BiometricPrompt dialog.</p>
 *
 * <p>The {@code authenticate} method blocks the calling thread via a
 * {@link CountDownLatch} until the prompt completes.</p>
 */
public final class GpuiLocalAuth {

    /** Latch that the calling (native) thread waits on. */
    static CountDownLatch sLatch;

    /** Authentication result code (see int constants below). */
    static AtomicInteger sResult = new AtomicInteger(7);

    /** Reason string displayed in the biometric prompt. */
    static String sReason;

    // Result codes (must match Rust int_to_auth_result)
    // 0 = success, 1 = failed, 2 = not_available, 3 = not_enrolled,
    // 4 = cancelled, 5 = passcode_not_set, 6 = lockout, 7 = other

    /**
     * Check if the device has biometric hardware.
     *
     * @return {@code true} if biometric hardware is present (even if not enrolled).
     */
    public static boolean isDeviceSupported(Activity activity) {
        if (Build.VERSION.SDK_INT >= 29) {
            android.hardware.biometrics.BiometricManager bm =
                activity.getSystemService(android.hardware.biometrics.BiometricManager.class);
            if (bm != null) {
                int result = bm.canAuthenticate();
                return result != android.hardware.biometrics.BiometricManager.BIOMETRIC_ERROR_HW_UNAVAILABLE
                    && result != android.hardware.biometrics.BiometricManager.BIOMETRIC_ERROR_NO_HARDWARE;
            }
        }
        // Fallback for API < 29: check for fingerprint hardware
        return activity.getPackageManager().hasSystemFeature("android.hardware.fingerprint");
    }

    /**
     * Check if biometrics are enrolled and ready to use.
     *
     * @return {@code true} if the user can authenticate right now.
     */
    public static boolean canAuthenticate(Activity activity) {
        if (Build.VERSION.SDK_INT >= 29) {
            android.hardware.biometrics.BiometricManager bm =
                activity.getSystemService(android.hardware.biometrics.BiometricManager.class);
            if (bm != null) {
                return bm.canAuthenticate() == android.hardware.biometrics.BiometricManager.BIOMETRIC_SUCCESS;
            }
        }
        return false;
    }

    /**
     * Get pipe-delimited list of available biometric types.
     *
     * @return e.g. "fingerprint|face" or "" if none.
     */
    public static String getAvailableBiometrics(Activity activity) {
        ArrayList<String> types = new ArrayList<>();
        if (activity.getPackageManager().hasSystemFeature("android.hardware.fingerprint")) {
            types.add("fingerprint");
        }
        if (activity.getPackageManager().hasSystemFeature("android.hardware.biometrics.face")) {
            types.add("face");
        }
        if (activity.getPackageManager().hasSystemFeature("android.hardware.biometrics.iris")) {
            types.add("iris");
        }
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < types.size(); i++) {
            if (i > 0) sb.append("|");
            sb.append(types.get(i));
        }
        return sb.toString();
    }

    /**
     * Authenticate the user with biometrics. Blocks until complete.
     *
     * <p>Launches {@link GpuiAuthActivity} which shows a BiometricPrompt.
     * The calling thread blocks on a {@link CountDownLatch} until the
     * prompt callback fires.</p>
     *
     * @param activity the current Activity context
     * @param reason   the reason string shown to the user
     * @return result code (0=success, 1=failed, 2=not_available, etc.)
     */
    public static int authenticate(Activity activity, String reason) {
        if (!canAuthenticate(activity)) {
            if (!isDeviceSupported(activity)) {
                return 2; // not available
            }
            return 3; // not enrolled
        }

        CountDownLatch latch = new CountDownLatch(1);
        sLatch = latch;
        sResult.set(7); // default: other error
        sReason = reason;

        // Launch GpuiAuthActivity which will show BiometricPrompt
        Intent intent = new Intent(activity, GpuiAuthActivity.class);
        intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
        activity.startActivity(intent);

        try {
            latch.await();
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            return 7; // other error
        }
        return sResult.get();
    }

    private GpuiLocalAuth() {}
}
