package dev.gpui.mobile;

import android.app.Activity;
import android.graphics.SurfaceTexture;
import android.media.MediaPlayer;
import android.media.PlaybackParams;
import android.net.Uri;
import android.os.Build;
import android.util.SparseArray;
import android.view.Surface;
import android.view.TextureView;
import android.view.ViewGroup;
import android.widget.FrameLayout;

import java.io.File;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicReference;

/**
 * Java helper for video playback using {@link MediaPlayer}.
 *
 * <p>All public methods are {@code static} and called from Rust via JNI.
 * Player instances are stored in a {@link SparseArray} keyed by integer ID.</p>
 */
public final class GpuiVideoPlayer {

    private static final String TAG = "GpuiVideoPlayer";

    private static final SparseArray<MediaPlayer> sPlayers = new SparseArray<>();
    private static final SparseArray<TextureView> sSurfaces = new SparseArray<>();
    private static int sNextId = 1;
    private static final Object sLock = new Object();

    /**
     * Create a new MediaPlayer instance.
     *
     * @param activity The current Activity.
     * @return A positive player ID, or -1 on failure.
     */
    public static int create(Activity activity) {
        try {
            MediaPlayer mp = new MediaPlayer();
            synchronized (sLock) {
                int id = sNextId++;
                sPlayers.put(id, mp);
                return id;
            }
        } catch (Exception e) {
            android.util.Log.e(TAG, "create failed", e);
            return -1;
        }
    }

    /**
     * Set a URL as the data source and prepare synchronously.
     *
     * @param activity The current Activity.
     * @param id       Player ID.
     * @param url      URL to play.
     * @return "duration|width|height" on success, or null on failure.
     */
    public static String setUrl(Activity activity, int id, String url) {
        MediaPlayer mp;
        synchronized (sLock) {
            mp = sPlayers.get(id);
        }
        if (mp == null) return null;

        try {
            mp.reset();
            mp.setDataSource(activity, Uri.parse(url));
            mp.prepare();
            return mp.getDuration() + "|" + mp.getVideoWidth() + "|" + mp.getVideoHeight();
        } catch (Exception e) {
            android.util.Log.e(TAG, "setUrl failed", e);
            return null;
        }
    }

    /**
     * Set a file path as the data source and prepare synchronously.
     *
     * @param activity The current Activity.
     * @param id       Player ID.
     * @param path     File path to play.
     * @return "duration|width|height" on success, or null on failure.
     */
    public static String setFilePath(Activity activity, int id, String path) {
        MediaPlayer mp;
        synchronized (sLock) {
            mp = sPlayers.get(id);
        }
        if (mp == null) return null;

        try {
            mp.reset();
            mp.setDataSource(path);
            mp.prepare();
            return mp.getDuration() + "|" + mp.getVideoWidth() + "|" + mp.getVideoHeight();
        } catch (Exception e) {
            android.util.Log.e(TAG, "setFilePath failed", e);
            return null;
        }
    }

    /**
     * Start or resume playback.
     */
    public static void play(int id) {
        MediaPlayer mp;
        synchronized (sLock) {
            mp = sPlayers.get(id);
        }
        if (mp != null) {
            try {
                mp.start();
            } catch (Exception e) {
                android.util.Log.e(TAG, "play failed", e);
            }
        }
    }

    /**
     * Pause playback.
     */
    public static void pause(int id) {
        MediaPlayer mp;
        synchronized (sLock) {
            mp = sPlayers.get(id);
        }
        if (mp != null) {
            try {
                mp.pause();
            } catch (Exception e) {
                android.util.Log.e(TAG, "pause failed", e);
            }
        }
    }

    /**
     * Seek to a position in milliseconds.
     */
    public static void seek(int id, long ms) {
        MediaPlayer mp;
        synchronized (sLock) {
            mp = sPlayers.get(id);
        }
        if (mp != null) {
            try {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                    mp.seekTo(ms, MediaPlayer.SEEK_CLOSEST);
                } else {
                    mp.seekTo((int) ms);
                }
            } catch (Exception e) {
                android.util.Log.e(TAG, "seek failed", e);
            }
        }
    }

    /**
     * Set volume (0.0 to 1.0).
     */
    public static void setVolume(int id, float volume) {
        MediaPlayer mp;
        synchronized (sLock) {
            mp = sPlayers.get(id);
        }
        if (mp != null) {
            try {
                mp.setVolume(volume, volume);
            } catch (Exception e) {
                android.util.Log.e(TAG, "setVolume failed", e);
            }
        }
    }

    /**
     * Set playback speed.
     */
    public static void setSpeed(int id, float speed) {
        MediaPlayer mp;
        synchronized (sLock) {
            mp = sPlayers.get(id);
        }
        if (mp != null) {
            try {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
                    PlaybackParams params = mp.getPlaybackParams();
                    params.setSpeed(speed);
                    mp.setPlaybackParams(params);
                }
            } catch (Exception e) {
                android.util.Log.e(TAG, "setSpeed failed", e);
            }
        }
    }

    /**
     * Enable or disable looping.
     */
    public static void setLooping(int id, boolean looping) {
        MediaPlayer mp;
        synchronized (sLock) {
            mp = sPlayers.get(id);
        }
        if (mp != null) {
            try {
                mp.setLooping(looping);
            } catch (Exception e) {
                android.util.Log.e(TAG, "setLooping failed", e);
            }
        }
    }

    /**
     * Get current playback position in milliseconds.
     */
    public static long getPosition(int id) {
        MediaPlayer mp;
        synchronized (sLock) {
            mp = sPlayers.get(id);
        }
        if (mp != null) {
            try {
                return mp.getCurrentPosition();
            } catch (Exception e) {
                android.util.Log.e(TAG, "getPosition failed", e);
            }
        }
        return 0;
    }

    /**
     * Get total duration in milliseconds.
     */
    public static long getDuration(int id) {
        MediaPlayer mp;
        synchronized (sLock) {
            mp = sPlayers.get(id);
        }
        if (mp != null) {
            try {
                return mp.getDuration();
            } catch (Exception e) {
                android.util.Log.e(TAG, "getDuration failed", e);
            }
        }
        return 0;
    }

    /**
     * Get video width in pixels.
     */
    public static int getWidth(int id) {
        MediaPlayer mp;
        synchronized (sLock) {
            mp = sPlayers.get(id);
        }
        if (mp != null) {
            try {
                return mp.getVideoWidth();
            } catch (Exception e) {
                android.util.Log.e(TAG, "getWidth failed", e);
            }
        }
        return 0;
    }

    /**
     * Get video height in pixels.
     */
    public static int getHeight(int id) {
        MediaPlayer mp;
        synchronized (sLock) {
            mp = sPlayers.get(id);
        }
        if (mp != null) {
            try {
                return mp.getVideoHeight();
            } catch (Exception e) {
                android.util.Log.e(TAG, "getHeight failed", e);
            }
        }
        return 0;
    }

    /**
     * Check if the player is currently playing.
     */
    public static boolean isPlaying(int id) {
        MediaPlayer mp;
        synchronized (sLock) {
            mp = sPlayers.get(id);
        }
        if (mp != null) {
            try {
                return mp.isPlaying();
            } catch (Exception e) {
                android.util.Log.e(TAG, "isPlaying failed", e);
            }
        }
        return false;
    }

    /**
     * Show a native TextureView surface at the given position and size (in px).
     * The MediaPlayer renders video frames to this surface.
     *
     * @param activity The current Activity.
     * @param id       Player ID.
     * @param x        Left position in pixels.
     * @param y        Top position in pixels.
     * @param width    Width in pixels.
     * @param height   Height in pixels.
     */
    public static void showSurface(Activity activity, int id, int x, int y, int width, int height) {
        MediaPlayer mp;
        synchronized (sLock) {
            mp = sPlayers.get(id);
        }
        if (mp == null) return;

        final MediaPlayer fmp = mp;
        final CountDownLatch latch = new CountDownLatch(1);

        activity.runOnUiThread(() -> {
            try {
                // Remove existing surface if any
                hideSurfaceInternal(id);

                TextureView tv = new TextureView(activity);
                FrameLayout.LayoutParams params = new FrameLayout.LayoutParams(width, height);
                params.leftMargin = x;
                params.topMargin = y;
                activity.addContentView(tv, params);

                synchronized (sLock) {
                    sSurfaces.put(id, tv);
                }

                tv.setSurfaceTextureListener(new TextureView.SurfaceTextureListener() {
                    @Override
                    public void onSurfaceTextureAvailable(SurfaceTexture surfaceTexture, int w, int h) {
                        Surface surface = new Surface(surfaceTexture);
                        try {
                            fmp.setSurface(surface);
                        } catch (Exception e) {
                            android.util.Log.e(TAG, "setSurface failed", e);
                        }
                        latch.countDown();
                    }

                    @Override
                    public void onSurfaceTextureSizeChanged(SurfaceTexture surface, int w, int h) {}

                    @Override
                    public boolean onSurfaceTextureDestroyed(SurfaceTexture surface) {
                        try {
                            fmp.setSurface(null);
                        } catch (Exception ignored) {}
                        return true;
                    }

                    @Override
                    public void onSurfaceTextureUpdated(SurfaceTexture surface) {}
                });

                // If texture is already available (reuse case)
                if (tv.isAvailable()) {
                    Surface surface = new Surface(tv.getSurfaceTexture());
                    try {
                        fmp.setSurface(surface);
                    } catch (Exception e) {
                        android.util.Log.e(TAG, "setSurface (immediate) failed", e);
                    }
                    latch.countDown();
                }
            } catch (Exception e) {
                android.util.Log.e(TAG, "showSurface failed", e);
                latch.countDown();
            }
        });

        try {
            latch.await(java.util.concurrent.TimeUnit.SECONDS.toMillis(3),
                        java.util.concurrent.TimeUnit.MILLISECONDS);
        } catch (InterruptedException ignored) {}
    }

    /**
     * Hide (remove) the native video surface overlay.
     *
     * @param activity The current Activity.
     * @param id       Player ID.
     */
    public static void hideSurface(Activity activity, int id) {
        final CountDownLatch latch = new CountDownLatch(1);
        activity.runOnUiThread(() -> {
            hideSurfaceInternal(id);
            latch.countDown();
        });
        try {
            latch.await(java.util.concurrent.TimeUnit.SECONDS.toMillis(2),
                        java.util.concurrent.TimeUnit.MILLISECONDS);
        } catch (InterruptedException ignored) {}
    }

    private static void hideSurfaceInternal(int id) {
        TextureView tv;
        synchronized (sLock) {
            tv = sSurfaces.get(id);
            sSurfaces.remove(id);
        }
        if (tv != null) {
            ViewGroup parent = (ViewGroup) tv.getParent();
            if (parent != null) {
                parent.removeView(tv);
            }
        }
    }

    /**
     * Release and remove the player.
     */
    public static void dispose(int id) {
        // Remove surface first
        hideSurfaceInternal(id);

        MediaPlayer mp;
        synchronized (sLock) {
            mp = sPlayers.get(id);
            sPlayers.remove(id);
        }
        if (mp != null) {
            try {
                mp.setSurface(null);
            } catch (Exception ignored) {
            }
            try {
                mp.stop();
            } catch (Exception ignored) {
            }
            try {
                mp.release();
            } catch (Exception ignored) {
            }
        }
    }

    // Prevent instantiation.
    private GpuiVideoPlayer() {}
}
