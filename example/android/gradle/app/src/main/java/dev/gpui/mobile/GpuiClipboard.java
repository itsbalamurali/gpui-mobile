package dev.gpui.mobile;

import android.app.Activity;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;

/**
 * Java helper for clipboard access on Android.
 *
 * <p>All public methods are {@code static} and called from Rust via JNI.</p>
 */
public final class GpuiClipboard {

    /**
     * Copy text to the clipboard.
     *
     * @param activity The current Activity.
     * @param text     The text to copy.
     */
    public static void setText(Activity activity, String text) {
        ClipboardManager cm = (ClipboardManager) activity.getSystemService(Context.CLIPBOARD_SERVICE);
        if (cm != null) {
            ClipData clip = ClipData.newPlainText("text", text);
            cm.setPrimaryClip(clip);
        }
    }

    /**
     * Read text from the clipboard.
     *
     * @param activity The current Activity.
     * @return The clipboard text, or {@code null} if empty or not text.
     */
    public static String getText(Activity activity) {
        ClipboardManager cm = (ClipboardManager) activity.getSystemService(Context.CLIPBOARD_SERVICE);
        if (cm == null || !cm.hasPrimaryClip()) return null;
        ClipData clip = cm.getPrimaryClip();
        if (clip == null || clip.getItemCount() == 0) return null;
        CharSequence text = clip.getItemAt(0).getText();
        return text != null ? text.toString() : null;
    }

    /**
     * Check if the clipboard has text content.
     *
     * @param activity The current Activity.
     * @return {@code true} if the clipboard contains text.
     */
    public static boolean hasText(Activity activity) {
        ClipboardManager cm = (ClipboardManager) activity.getSystemService(Context.CLIPBOARD_SERVICE);
        if (cm == null || !cm.hasPrimaryClip()) return false;
        ClipData clip = cm.getPrimaryClip();
        return clip != null && clip.getItemCount() > 0 && clip.getItemAt(0).getText() != null;
    }

    private GpuiClipboard() {}
}
