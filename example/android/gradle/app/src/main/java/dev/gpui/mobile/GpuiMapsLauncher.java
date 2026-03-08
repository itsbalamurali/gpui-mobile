package dev.gpui.mobile;

import android.app.Activity;
import android.content.Intent;
import android.net.Uri;

public final class GpuiMapsLauncher {

    public static boolean openCoordinates(Activity activity, double lat, double lon, String label) {
        try {
            String uri;
            if (label != null && !label.isEmpty()) {
                uri = String.format("geo:%f,%f?q=%f,%f(%s)", lat, lon, lat, lon,
                    Uri.encode(label));
            } else {
                uri = String.format("geo:%f,%f?q=%f,%f", lat, lon, lat, lon);
            }
            Intent intent = new Intent(Intent.ACTION_VIEW, Uri.parse(uri));
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            activity.startActivity(intent);
            return true;
        } catch (Exception e) {
            return false;
        }
    }

    public static boolean openQuery(Activity activity, String query) {
        try {
            String uri = "geo:0,0?q=" + Uri.encode(query);
            Intent intent = new Intent(Intent.ACTION_VIEW, Uri.parse(uri));
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            activity.startActivity(intent);
            return true;
        } catch (Exception e) {
            return false;
        }
    }

    public static boolean openDirections(Activity activity, double lat, double lon, String label) {
        try {
            String dest = lat + "," + lon;
            if (label != null && !label.isEmpty()) {
                dest = Uri.encode(label) + "@" + dest;
            }
            String uri = "google.navigation:q=" + dest;
            Intent intent = new Intent(Intent.ACTION_VIEW, Uri.parse(uri));
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            // Try Google Maps first
            intent.setPackage("com.google.android.apps.maps");
            try {
                activity.startActivity(intent);
                return true;
            } catch (Exception e) {
                // Fallback to generic geo intent
                intent = new Intent(Intent.ACTION_VIEW,
                    Uri.parse("geo:" + lat + "," + lon + "?q=" + lat + "," + lon));
                intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
                activity.startActivity(intent);
                return true;
            }
        } catch (Exception e) {
            return false;
        }
    }

    public static boolean isAvailable(Activity activity) {
        Intent intent = new Intent(Intent.ACTION_VIEW, Uri.parse("geo:0,0"));
        return intent.resolveActivity(activity.getPackageManager()) != null;
    }

    private GpuiMapsLauncher() {}
}
