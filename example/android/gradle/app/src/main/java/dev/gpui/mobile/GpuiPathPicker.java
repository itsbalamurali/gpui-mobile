package dev.gpui.mobile;

import android.app.Activity;
import android.content.Intent;
import android.database.Cursor;
import android.net.Uri;
import android.os.Bundle;
import android.os.Looper;
import android.provider.OpenableColumns;

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.util.ArrayList;
import java.util.UUID;
import java.util.concurrent.CountDownLatch;

/** GPUI file prompts import SAF documents into cache before releasing URI access. */
public final class GpuiPathPicker extends Activity {
    private static final int PICK_FILES = 9010;
    private static Request pending;
    private Request request;

    private static final class Request {
        final CountDownLatch done = new CountDownLatch(1);
        final boolean multiple;
        String[] paths;
        Exception error;

        Request(boolean multiple) { this.multiple = multiple; }

        synchronized void complete(String[] paths, Exception error) {
            if (done.getCount() == 0) return;
            this.paths = paths;
            this.error = error;
            done.countDown();
        }
    }

    /** Called by a Rust worker, never the UI or GPUI event thread. */
    public static String[] openFiles(Activity activity, boolean multiple) throws Exception {
        if (Looper.myLooper() == Looper.getMainLooper()) {
            throw new IllegalStateException("File prompts must run on a worker thread");
        }
        final Request current = new Request(multiple);
        synchronized (GpuiPathPicker.class) {
            if (pending != null) throw new IllegalStateException("A file picker is already open");
            pending = current;
        }
        try {
            activity.runOnUiThread(() -> {
                try {
                    activity.startActivity(new Intent(activity, GpuiPathPicker.class));
                } catch (Exception error) {
                    current.complete(null, error);
                }
            });
            current.done.await();
            if (current.error != null) throw current.error;
            return current.paths;
        } finally {
            synchronized (GpuiPathPicker.class) {
                if (pending == current) pending = null;
            }
        }
    }

    @Override protected void onCreate(Bundle state) {
        super.onCreate(state);
        synchronized (GpuiPathPicker.class) { request = pending; }
        // After process death the original GPUI request no longer exists.
        if (request == null) { finish(); return; }
        if (state != null) return;
        Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
        intent.addCategory(Intent.CATEGORY_OPENABLE);
        intent.setType("*/*");
        intent.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, request.multiple);
        intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
        try {
            startActivityForResult(intent, PICK_FILES);
        } catch (Exception error) {
            request.complete(null, error);
            finish();
        }
    }

    @Override protected void onActivityResult(int code, int result, Intent data) {
        super.onActivityResult(code, result, data);
        if (code != PICK_FILES || request == null) return;
        if (result != RESULT_OK || data == null) {
            request.complete(null, null);
            finish();
            return;
        }
        ArrayList<Uri> uris = new ArrayList<>();
        if (data.getClipData() != null) {
            for (int i = 0; i < data.getClipData().getItemCount(); i++) {
                Uri uri = data.getClipData().getItemAt(i).getUri();
                if (uri != null) uris.add(uri);
            }
        } else if (data.getData() != null) {
            uris.add(data.getData());
        }
        // Keep this Activity (and its temporary URI grants) alive while reading.
        // Files from remote providers can be large, so copying cannot run on UI.
        new Thread(() -> {
            ArrayList<File> imported = new ArrayList<>();
            try {
                for (Uri uri : uris) imported.add(importFile(uri));
                String[] paths = new String[imported.size()];
                for (int i = 0; i < paths.length; i++) paths[i] = imported.get(i).getAbsolutePath();
                request.complete(paths, null);
            } catch (Exception error) {
                for (File file : imported) {
                    file.delete();
                    file.getParentFile().delete();
                }
                request.complete(null, error);
            } finally {
                runOnUiThread(this::finish);
            }
        }, "gpui-file-import").start();
    }

    private File importFile(Uri uri) throws IOException {
        String name = null;
        try (Cursor cursor = getContentResolver().query(uri,
                new String[] { OpenableColumns.DISPLAY_NAME }, null, null, null)) {
            if (cursor != null && cursor.moveToFirst()) name = cursor.getString(0);
        }
        // Keep the original extension and Unicode filename for attachment checks,
        // but never allow a provider's display name to escape the import directory.
        if (name == null || name.isEmpty()) name = uri.getLastPathSegment();
        if (name == null) name = "attachment";
        name = name.replace('/', '_').replace('\\', '_').replace('\0', '_');
        if (name.isEmpty() || name.equals(".") || name.equals("..")) name = "attachment";
        File directory = new File(getCacheDir(), "gpui-import-" + UUID.randomUUID());
        if (!directory.mkdir()) throw new IOException("Could not create import directory");
        File file = new File(directory, name);
        try (InputStream input = getContentResolver().openInputStream(uri);
             FileOutputStream output = new FileOutputStream(file)) {
            if (input == null) throw new IOException("Could not read selected document");
            byte[] buffer = new byte[64 * 1024];
            int count;
            while ((count = input.read(buffer)) != -1) output.write(buffer, 0, count);
        } catch (Exception error) {
            file.delete();
            directory.delete();
            throw error;
        }
        return file;
    }

    @Override protected void onDestroy() {
        if (isFinishing() && request != null) request.complete(null, null);
        super.onDestroy();
    }
}
