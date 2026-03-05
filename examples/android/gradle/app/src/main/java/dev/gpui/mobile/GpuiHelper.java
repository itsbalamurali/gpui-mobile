package dev.gpui.mobile;

import android.app.Activity;
import android.graphics.Color;
import android.os.Handler;
import android.os.Looper;
import android.view.View;
import android.view.ViewGroup;
import android.webkit.JavascriptInterface;
import android.webkit.WebChromeClient;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.FrameLayout;

import java.util.concurrent.CountDownLatch;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicReference;

/**
 * Java helper for operations that require the Android UI thread.
 *
 * <p>Android's {@link WebView} and IME (keyboard) APIs must be called from
 * the UI thread. Since GPUI's native Rust code runs on a dedicated native
 * thread, we use this helper class with {@code runOnUiThread} to bridge
 * the gap.</p>
 *
 * <p>All public methods are {@code static} and called from Rust via JNI.</p>
 */
public final class GpuiHelper {

    // ── WebView management ──────────────────────────────────────────────

    /** The single active WebView instance (if any). */
    private static WebView sWebView;
    /** Global ID counter for WebView handles. */
    private static final AtomicInteger sNextId = new AtomicInteger(1);
    /** Handler for posting to the main/UI thread. */
    private static final Handler sMainHandler = new Handler(Looper.getMainLooper());

    /**
     * Load a URL in an in-app WebView.
     *
     * <p>Blocks the calling thread until the WebView is created on the UI
     * thread and the URL load has been initiated.</p>
     *
     * @param activity The current Activity.
     * @param url      The URL to load.
     * @param js       Enable JavaScript.
     * @param dom      Enable DOM storage.
     * @param zoom     Enable built-in zoom controls.
     * @return A positive integer handle, or -1 on error.
     */
    public static int loadUrl(final Activity activity, final String url,
                              final boolean js, final boolean dom, final boolean zoom) {
        final CountDownLatch latch = new CountDownLatch(1);
        final AtomicInteger result = new AtomicInteger(-1);

        activity.runOnUiThread(() -> {
            try {
                WebView wv = createWebView(activity, js, dom, zoom);
                wv.loadUrl(url);
                result.set(sNextId.getAndIncrement());
            } catch (Exception e) {
                android.util.Log.e("GpuiHelper", "loadUrl failed", e);
            } finally {
                latch.countDown();
            }
        });

        try {
            latch.await();
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            return -1;
        }
        return result.get();
    }

    /**
     * Load raw HTML content in an in-app WebView.
     *
     * @param activity The current Activity.
     * @param html     The HTML string.
     * @param js       Enable JavaScript.
     * @param dom      Enable DOM storage.
     * @param zoom     Enable built-in zoom controls.
     * @return A positive integer handle, or -1 on error.
     */
    public static int loadHtml(final Activity activity, final String html,
                               final boolean js, final boolean dom, final boolean zoom) {
        final CountDownLatch latch = new CountDownLatch(1);
        final AtomicInteger result = new AtomicInteger(-1);

        activity.runOnUiThread(() -> {
            try {
                WebView wv = createWebView(activity, js, dom, zoom);
                wv.loadDataWithBaseURL(null, html, "text/html", "UTF-8", null);
                result.set(sNextId.getAndIncrement());
            } catch (Exception e) {
                android.util.Log.e("GpuiHelper", "loadHtml failed", e);
            } finally {
                latch.countDown();
            }
        });

        try {
            latch.await();
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            return -1;
        }
        return result.get();
    }

    /**
     * Evaluate JavaScript in the active WebView.
     *
     * @param script The JavaScript code to execute.
     */
    public static void evaluateJavascript(final String script) {
        sMainHandler.post(() -> {
            if (sWebView != null) {
                sWebView.evaluateJavascript(script, null);
            }
        });
    }

    /**
     * Dismiss (remove and destroy) the active WebView.
     *
     * @param activity The current Activity.
     */
    public static void dismissWebView(final Activity activity) {
        activity.runOnUiThread(() -> {
            if (sWebView != null) {
                ViewGroup parent = (ViewGroup) sWebView.getParent();
                if (parent != null) {
                    parent.removeView(sWebView);
                }
                sWebView.destroy();
                sWebView = null;
            }
        });
    }

    // ── internal helpers ─────────────────────────────────────────────────

    /**
     * Create a full-screen WebView and add it to the Activity's content view.
     *
     * <p>Must be called on the UI thread.</p>
     */
    private static WebView createWebView(Activity activity, boolean js,
                                         boolean dom, boolean zoom) {
        // Dismiss any existing WebView first.
        if (sWebView != null) {
            ViewGroup parent = (ViewGroup) sWebView.getParent();
            if (parent != null) {
                parent.removeView(sWebView);
            }
            sWebView.destroy();
            sWebView = null;
        }

        WebView wv = new WebView(activity);

        WebSettings settings = wv.getSettings();
        settings.setJavaScriptEnabled(js);
        settings.setDomStorageEnabled(dom);
        settings.setBuiltInZoomControls(zoom);
        if (zoom) {
            settings.setDisplayZoomControls(false); // hide +/- buttons
        }
        settings.setLoadWithOverviewMode(true);
        settings.setUseWideViewPort(true);

        wv.setWebViewClient(new WebViewClient());
        wv.setWebChromeClient(new WebChromeClient());
        wv.setBackgroundColor(Color.WHITE);

        // Add as a full-screen overlay.
        FrameLayout.LayoutParams params = new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.MATCH_PARENT);
        activity.addContentView(wv, params);

        sWebView = wv;
        return wv;
    }

    // Prevent instantiation.
    private GpuiHelper() {}
}
