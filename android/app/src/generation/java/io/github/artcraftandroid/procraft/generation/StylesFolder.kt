package io.github.artcraftandroid.procraft.generation

import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Environment
import android.provider.DocumentsContract
import android.provider.MediaStore
import io.github.artcraftandroid.procraft.core.StyleFileText
import java.io.File

/**
 * The person's styles folder, where a person can see it: Documents › ProCraft Diffusion › Styles
 * in the tablet's shared storage by default, seen in the Files app and from a PC, kept after an
 * uninstall. The app makes the folder itself (any app may add its own files to Documents), then
 * Android's folder picker, opened inside it, lets the person give the app the whole folder once,
 * so it sees every file there, those added by hand or by a sync tool included. The grant is kept
 * across restarts; the folder can be changed to any other the same way.
 *
 * Rust can't use Android's document API, so the files are read here and their text handed to the
 * core.
 */
internal object StylesFolder {
    /** The default folder under shared storage, as MediaStore and the picker name it. */
    private const val DEFAULT = "Documents/ProCraft Diffusion/Styles"
    private const val KEY = "styles_folder"
    /** A file bigger than this isn't a style. */
    private const val MOST_BYTES = 256L * 1024
    private const val ABOUT = "About these files.txt"
    private const val ABOUT_TEXT =
        "This folder holds your ProCraft Diffusion styles: one .toml file for each style, named after it.\n\n" +
            "They're plain text. Open one in any text editor to read or change it. A style added here " +
            "(copied from another device, or brought by a sync app) shows up the next time Generate opens.\n"

    private fun prefs(context: Context) = context.getSharedPreferences("generation", Context.MODE_PRIVATE)

    /** The folder the app was given, if it still has it. */
    fun tree(context: Context): Uri? {
        val kept = prefs(context).getString(KEY, null)?.let(Uri::parse) ?: return null
        val granted = context.contentResolver.persistedUriPermissions.any { it.uri == kept && it.isReadPermission }
        return kept.takeIf { granted }
    }

    /** The folder as a person reads it ("Documents/ProCraft Diffusion/Styles"); none if not chosen. */
    fun shownName(context: Context): String? {
        val tree = tree(context) ?: return null
        val id = runCatching { DocumentsContract.getTreeDocumentId(tree) }.getOrNull() ?: return null
        return id.substringAfter(':', id).ifEmpty { "the storage's top" }
    }

    /**
     * Makes the default folder if it isn't there, with a short note saying what the files are (a
     * folder can only be made with a file in it), and returns where the picker should open.
     */
    fun prepare(context: Context): Uri {
        val dir = File(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOCUMENTS), "ProCraft Diffusion/Styles")
        if (!dir.isDirectory) {
            runCatching {
                val values = ContentValues().apply {
                    put(MediaStore.MediaColumns.DISPLAY_NAME, ABOUT)
                    put(MediaStore.MediaColumns.MIME_TYPE, "text/plain")
                    put(MediaStore.MediaColumns.RELATIVE_PATH, "$DEFAULT/")
                }
                val made = context.contentResolver.insert(MediaStore.Files.getContentUri("external"), values)
                made?.let { context.contentResolver.openOutputStream(it)?.use { out -> out.write(ABOUT_TEXT.toByteArray()) } }
            }
        }
        return DocumentsContract.buildDocumentUri("com.android.externalstorage.documents", "primary:$DEFAULT")
    }

    /** The person chose `tree` in the picker: the app keeps it, and its grant. */
    fun chosen(context: Context, tree: Uri) {
        val flags = Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION
        runCatching { context.contentResolver.takePersistableUriPermission(tree, flags) }
        val old = prefs(context).getString(KEY, null)?.let(Uri::parse)
        if (old != null && old != tree) runCatching { context.contentResolver.releasePersistableUriPermission(old, flags) }
        prefs(context).edit().putString(KEY, tree.toString()).apply()
    }

    /** The folder's style files (each `.toml` in it, not in folders below), by name: through the
     *  folder granted, else the ones the app itself wrote into the default folder. */
    fun read(context: Context): List<StyleFileText> {
        val resolver = context.contentResolver
        return list(context).mapNotNull { (name, uri) ->
            runCatching { resolver.openInputStream(uri)?.use { String(it.readBytes()) } }.getOrNull()?.let { StyleFileText(name, it) }
        }.sortedBy { it.name }
    }

    /** The style files there are and where each is. */
    private fun list(context: Context): List<Pair<String, Uri>> {
        val resolver = context.contentResolver
        val tree = tree(context)
        val found = mutableListOf<Pair<String, Uri>>()
        runCatching {
            if (tree != null) {
                val children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, DocumentsContract.getTreeDocumentId(tree))
                val columns = arrayOf(
                    DocumentsContract.Document.COLUMN_DOCUMENT_ID,
                    DocumentsContract.Document.COLUMN_DISPLAY_NAME,
                    DocumentsContract.Document.COLUMN_SIZE,
                    DocumentsContract.Document.COLUMN_MIME_TYPE,
                )
                resolver.query(children, columns, null, null, null)?.use { c ->
                    while (c.moveToNext()) {
                        val name = c.getString(1) ?: continue
                        val folder = c.getString(3) == DocumentsContract.Document.MIME_TYPE_DIR
                        if (folder || !name.endsWith(".toml", ignoreCase = true) || c.getLong(2) > MOST_BYTES) continue
                        found += name to DocumentsContract.buildDocumentUriUsingTree(tree, c.getString(0))
                    }
                }
            } else {
                // without the grant: the files the app wrote there itself, which MediaStore shows it
                val files = MediaStore.Files.getContentUri("external")
                val columns = arrayOf(MediaStore.MediaColumns._ID, MediaStore.MediaColumns.DISPLAY_NAME)
                resolver.query(files, columns, "${MediaStore.MediaColumns.RELATIVE_PATH} = ?", arrayOf("$DEFAULT/"), null)?.use { c ->
                    while (c.moveToNext()) {
                        val name = c.getString(1) ?: continue
                        if (name.endsWith(".toml", ignoreCase = true)) found += name to android.content.ContentUris.withAppendedId(files, c.getLong(0))
                    }
                }
            }
        }
        return found
    }

    /** Whether the person has given the app a folder (else it asks, after a style is saved). */
    fun granted(context: Context): Boolean = tree(context) != null

    /**
     * Writes `text` as `name` (`Moonlit.toml`): over the file of that name if there is one, else a
     * new one. `replacing` is the style's old file, removed when the style was renamed. The
     * reason it couldn't, if it couldn't.
     */
    fun write(context: Context, name: String, text: String, replacing: String?): String? {
        val resolver = context.contentResolver
        return runCatching {
            val there = list(context)
            val target = there.firstOrNull { it.first == name }?.second ?: run {
                val tree = tree(context)
                if (tree != null) {
                    val folder = DocumentsContract.buildDocumentUriUsingTree(tree, DocumentsContract.getTreeDocumentId(tree))
                    DocumentsContract.createDocument(resolver, folder, "application/octet-stream", name)
                } else {
                    val values = ContentValues().apply {
                        put(MediaStore.MediaColumns.DISPLAY_NAME, name)
                        put(MediaStore.MediaColumns.MIME_TYPE, "application/octet-stream")
                        put(MediaStore.MediaColumns.RELATIVE_PATH, "$DEFAULT/")
                    }
                    resolver.insert(MediaStore.Files.getContentUri("external"), values)
                }
            } ?: return "the folder wouldn't take a new file"
            resolver.openOutputStream(target, "wt")?.use { it.write(text.toByteArray()) } ?: return "the file couldn't be opened"
            if (replacing != null && replacing != name) delete(context, replacing)
            null
        }.getOrElse { it.message ?: it.toString() }
    }

    /** Removes the style file `name`; the reason it couldn't, if it couldn't. */
    fun delete(context: Context, name: String): String? {
        val uri = list(context).firstOrNull { it.first == name }?.second ?: return null
        return runCatching {
            if (tree(context) != null) DocumentsContract.deleteDocument(context.contentResolver, uri) else context.contentResolver.delete(uri, null, null)
            null
        }.getOrElse { it.message ?: it.toString() }
    }
}
