package dev.gpui.mobile;

import android.app.Activity;
import android.content.ContentResolver;
import android.database.Cursor;
import android.provider.ContactsContract;

/**
 * JNI helper for reading the device address book via ContactsContract.
 *
 * Returns contacts in a pipe/newline-delimited text format so the Rust side
 * can parse them without a JSON dependency:
 *
 * Each line: id|displayName|givenName|familyName|phone1:label1,phone2:label2|email1:label1,email2:label2
 */
public final class GpuiContacts {

    /**
     * Get all contacts, sorted by display name.
     */
    public static String getContacts(Activity activity) {
        return queryContacts(activity, null, null);
    }

    /**
     * Search contacts by display name (case-insensitive LIKE match).
     */
    public static String searchContacts(Activity activity, String query) {
        return queryContacts(activity,
            ContactsContract.Contacts.DISPLAY_NAME_PRIMARY + " LIKE ?",
            new String[]{"%" + query + "%"});
    }

    /**
     * Get a single contact by its _ID.
     */
    public static String getContact(Activity activity, String id) {
        return queryContacts(activity,
            ContactsContract.Contacts._ID + " = ?",
            new String[]{id});
    }

    private static String queryContacts(Activity activity, String selection, String[] selectionArgs) {
        ContentResolver cr = activity.getContentResolver();
        StringBuilder result = new StringBuilder();

        Cursor cursor = cr.query(
            ContactsContract.Contacts.CONTENT_URI,
            new String[]{
                ContactsContract.Contacts._ID,
                ContactsContract.Contacts.DISPLAY_NAME_PRIMARY,
            },
            selection, selectionArgs,
            ContactsContract.Contacts.DISPLAY_NAME_PRIMARY + " ASC"
        );

        if (cursor == null) return "";

        try {
            while (cursor.moveToNext()) {
                String contactId = cursor.getString(0);
                String displayName = cursor.getString(1);
                if (displayName == null) displayName = "";

                // Escape pipe characters in display name to avoid parsing issues
                displayName = escapePipe(displayName);

                // Get structured name
                String givenName = "";
                String familyName = "";
                Cursor nameCursor = cr.query(
                    ContactsContract.Data.CONTENT_URI,
                    new String[]{
                        ContactsContract.CommonDataKinds.StructuredName.GIVEN_NAME,
                        ContactsContract.CommonDataKinds.StructuredName.FAMILY_NAME,
                    },
                    ContactsContract.Data.CONTACT_ID + " = ? AND " +
                        ContactsContract.Data.MIMETYPE + " = ?",
                    new String[]{contactId, ContactsContract.CommonDataKinds.StructuredName.CONTENT_ITEM_TYPE},
                    null
                );
                if (nameCursor != null) {
                    try {
                        if (nameCursor.moveToFirst()) {
                            givenName = nameCursor.getString(0);
                            familyName = nameCursor.getString(1);
                            if (givenName == null) givenName = "";
                            if (familyName == null) familyName = "";
                            givenName = escapePipe(givenName);
                            familyName = escapePipe(familyName);
                        }
                    } finally {
                        nameCursor.close();
                    }
                }

                // Get phone numbers
                StringBuilder phones = new StringBuilder();
                Cursor phoneCursor = cr.query(
                    ContactsContract.CommonDataKinds.Phone.CONTENT_URI,
                    new String[]{
                        ContactsContract.CommonDataKinds.Phone.NUMBER,
                        ContactsContract.CommonDataKinds.Phone.TYPE,
                    },
                    ContactsContract.CommonDataKinds.Phone.CONTACT_ID + " = ?",
                    new String[]{contactId}, null
                );
                if (phoneCursor != null) {
                    try {
                        while (phoneCursor.moveToNext()) {
                            if (phones.length() > 0) phones.append(",");
                            String number = phoneCursor.getString(0);
                            int type = phoneCursor.getInt(1);
                            String label = ContactsContract.CommonDataKinds.Phone.getTypeLabel(
                                activity.getResources(), type, "other").toString();
                            number = escapeDelimiters(number != null ? number : "");
                            label = escapeDelimiters(label);
                            phones.append(number).append(":").append(label);
                        }
                    } finally {
                        phoneCursor.close();
                    }
                }

                // Get emails
                StringBuilder emails = new StringBuilder();
                Cursor emailCursor = cr.query(
                    ContactsContract.CommonDataKinds.Email.CONTENT_URI,
                    new String[]{
                        ContactsContract.CommonDataKinds.Email.ADDRESS,
                        ContactsContract.CommonDataKinds.Email.TYPE,
                    },
                    ContactsContract.CommonDataKinds.Email.CONTACT_ID + " = ?",
                    new String[]{contactId}, null
                );
                if (emailCursor != null) {
                    try {
                        while (emailCursor.moveToNext()) {
                            if (emails.length() > 0) emails.append(",");
                            String addr = emailCursor.getString(0);
                            int type = emailCursor.getInt(1);
                            String label = ContactsContract.CommonDataKinds.Email.getTypeLabel(
                                activity.getResources(), type, "other").toString();
                            addr = escapeDelimiters(addr != null ? addr : "");
                            label = escapeDelimiters(label);
                            emails.append(addr).append(":").append(label);
                        }
                    } finally {
                        emailCursor.close();
                    }
                }

                if (result.length() > 0) result.append("\n");
                result.append(contactId).append("|")
                      .append(displayName).append("|")
                      .append(givenName).append("|")
                      .append(familyName).append("|")
                      .append(phones).append("|")
                      .append(emails);
            }
        } finally {
            cursor.close();
        }

        return result.toString();
    }

    /** Escape pipe characters so they don't break field splitting. */
    private static String escapePipe(String s) {
        if (s == null) return "";
        return s.replace("|", " ");
    }

    /** Escape colon and comma characters so they don't break phone/email sub-field splitting. */
    private static String escapeDelimiters(String s) {
        if (s == null) return "";
        return s.replace("|", " ").replace(",", " ").replace(":", " ");
    }

    private GpuiContacts() {}
}
