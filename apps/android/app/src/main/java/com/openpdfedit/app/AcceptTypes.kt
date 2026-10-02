package com.openpdfedit.app

/**
 * Turns an `<input accept="…">` list into MIME types a document picker
 * will accept.
 *
 * The web layer writes `accept="application/pdf,.pdf"`, which is correct
 * HTML — a file input takes extensions and MIME types interchangeably.
 * `Intent.EXTRA_MIME_TYPES` does not: every entry has to be a MIME type,
 * and one that is not poisons the whole filter rather than being
 * ignored. Passing the list straight through therefore greyed out every
 * file in the picker, including the PDFs the app exists to open, with
 * nothing on screen to suggest the app had asked for the wrong thing.
 *
 * Extensions are mapped where Android knows them and dropped where it
 * does not. An empty result falls back to everything rather than to
 * `application/pdf`: this chooser also serves the watermark logo picker,
 * and quietly narrowing that to PDFs would be the same bug with a
 * different victim.
 *
 * `mimeForExtension` is a parameter rather than a direct call to
 * `MimeTypeMap.getSingleton()` so this function is a plain Kotlin one
 * and can be tested without a device.
 */
internal fun mimeTypesFor(
    acceptTypes: Array<String>,
    mimeForExtension: (String) -> String?,
): Array<String> {
    val types = acceptTypes
        .flatMap { it.split(",") }
        .map { it.trim() }
        .filter { it.isNotEmpty() }
        .mapNotNull { entry ->
            if (entry.contains("/")) entry
            else mimeForExtension(entry.removePrefix(".").lowercase())
        }
        .distinct()
    return if (types.isEmpty()) arrayOf("*/*") else types.toTypedArray()
}
