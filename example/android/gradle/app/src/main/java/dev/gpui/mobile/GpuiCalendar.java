package dev.gpui.mobile;

import android.app.Activity;
import android.content.ContentResolver;
import android.content.ContentValues;
import android.database.Cursor;
import android.net.Uri;
import android.provider.CalendarContract;

import java.util.TimeZone;

public final class GpuiCalendar {

    public static String getCalendars(Activity activity) {
        ContentResolver cr = activity.getContentResolver();
        StringBuilder result = new StringBuilder();

        Cursor cursor = null;
        try {
            cursor = cr.query(
                CalendarContract.Calendars.CONTENT_URI,
                new String[]{
                    CalendarContract.Calendars._ID,
                    CalendarContract.Calendars.CALENDAR_DISPLAY_NAME,
                    CalendarContract.Calendars.CALENDAR_ACCESS_LEVEL,
                    CalendarContract.Calendars.CALENDAR_COLOR,
                },
                null, null, null
            );

            if (cursor == null) return "";
            while (cursor.moveToNext()) {
                if (result.length() > 0) result.append("\n");
                String id = cursor.getString(0);
                String name = cursor.getString(1);
                int access = cursor.getInt(2);
                int color = cursor.getInt(3);
                boolean readOnly = access < CalendarContract.Calendars.CAL_ACCESS_CONTRIBUTOR;
                result.append(id).append("|")
                      .append(name != null ? name : "").append("|")
                      .append(readOnly ? "1" : "0").append("|")
                      .append(color & 0xFFFFFFFFL);
            }
        } catch (Exception e) {
            android.util.Log.e("GpuiCalendar", "getCalendars failed", e);
        } finally {
            if (cursor != null) cursor.close();
        }
        return result.toString();
    }

    public static String getEvents(Activity activity, String calendarId, long startMs, long endMs) {
        ContentResolver cr = activity.getContentResolver();
        StringBuilder result = new StringBuilder();

        String selection = CalendarContract.Events.CALENDAR_ID + " = ? AND " +
            CalendarContract.Events.DTSTART + " >= ? AND " +
            CalendarContract.Events.DTSTART + " <= ?";

        Cursor cursor = null;
        try {
            cursor = cr.query(
                CalendarContract.Events.CONTENT_URI,
                new String[]{
                    CalendarContract.Events._ID,
                    CalendarContract.Events.TITLE,
                    CalendarContract.Events.DESCRIPTION,
                    CalendarContract.Events.EVENT_LOCATION,
                    CalendarContract.Events.DTSTART,
                    CalendarContract.Events.DTEND,
                    CalendarContract.Events.ALL_DAY,
                    CalendarContract.Events.CALENDAR_ID,
                },
                selection,
                new String[]{calendarId, String.valueOf(startMs), String.valueOf(endMs)},
                CalendarContract.Events.DTSTART + " ASC"
            );

            if (cursor == null) return "";
            while (cursor.moveToNext()) {
                if (result.length() > 0) result.append("\n");
                result.append(cursor.getString(0)).append("|")  // id
                      .append(nullSafe(cursor.getString(1))).append("|")  // title
                      .append(nullSafe(cursor.getString(2))).append("|")  // description
                      .append(nullSafe(cursor.getString(3))).append("|")  // location
                      .append(cursor.getLong(4)).append("|")   // startMs
                      .append(cursor.getLong(5)).append("|")   // endMs
                      .append(cursor.getInt(6) != 0 ? "1" : "0").append("|") // allDay
                      .append(cursor.getString(7));  // calendarId
            }
        } catch (Exception e) {
            android.util.Log.e("GpuiCalendar", "getEvents failed", e);
        } finally {
            if (cursor != null) cursor.close();
        }
        return result.toString();
    }

    public static String createEvent(Activity activity, String calendarId,
            String title, String description, String location,
            long startMs, long endMs, boolean allDay) {
        ContentValues values = new ContentValues();
        values.put(CalendarContract.Events.CALENDAR_ID, Long.parseLong(calendarId));
        values.put(CalendarContract.Events.TITLE, title);
        values.put(CalendarContract.Events.DESCRIPTION, description);
        values.put(CalendarContract.Events.EVENT_LOCATION, location);
        values.put(CalendarContract.Events.DTSTART, startMs);
        values.put(CalendarContract.Events.DTEND, endMs);
        values.put(CalendarContract.Events.ALL_DAY, allDay ? 1 : 0);
        values.put(CalendarContract.Events.EVENT_TIMEZONE, TimeZone.getDefault().getID());

        try {
            Uri uri = activity.getContentResolver().insert(CalendarContract.Events.CONTENT_URI, values);
            if (uri != null) {
                return uri.getLastPathSegment();
            }
        } catch (Exception e) {
            android.util.Log.e("GpuiCalendar", "createEvent failed", e);
        }
        return null;
    }

    public static boolean deleteEvent(Activity activity, String eventId) {
        try {
            Uri uri = CalendarContract.Events.CONTENT_URI.buildUpon()
                .appendPath(eventId).build();
            int rows = activity.getContentResolver().delete(uri, null, null);
            return rows > 0;
        } catch (Exception e) {
            android.util.Log.e("GpuiCalendar", "deleteEvent failed", e);
            return false;
        }
    }

    private static String nullSafe(String s) { return s != null ? s : ""; }

    private GpuiCalendar() {}
}
