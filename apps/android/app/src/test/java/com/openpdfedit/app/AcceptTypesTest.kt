package com.openpdfedit.app

import org.junit.Assert.assertArrayEquals
import org.junit.Test

class AcceptTypesTest {

    /** Android's table, as far as these tests need it. */
    private val table = { ext: String ->
        when (ext) {
            "pdf" -> "application/pdf"
            "png" -> "image/png"
            else -> null
        }
    }

    @Test
    fun `an extension alongside a mime type does not poison the filter`() {
        // What the web layer actually sends. Before this was mapped, the
        // bare `.pdf` went into EXTRA_MIME_TYPES as-is and the picker
        // greyed out every file on the device, PDFs included.
        assertArrayEquals(
            arrayOf("application/pdf"),
            mimeTypesFor(arrayOf("application/pdf,.pdf"), table),
        )
    }

    @Test
    fun `one accept entry per array slot is treated the same as a comma list`() {
        assertArrayEquals(
            arrayOf("application/pdf"),
            mimeTypesFor(arrayOf("application/pdf", ".pdf"), table),
        )
    }

    @Test
    fun `an extension Android does not know is dropped, not passed through`() {
        assertArrayEquals(
            arrayOf("image/png"),
            mimeTypesFor(arrayOf(".png,.sketch"), table),
        )
    }

    @Test
    fun `nothing recognised means everything, not nothing`() {
        // The watermark logo picker shares this chooser. Falling back to
        // application/pdf would grey out every image instead.
        assertArrayEquals(arrayOf("*/*"), mimeTypesFor(arrayOf(".sketch"), table))
        assertArrayEquals(arrayOf("*/*"), mimeTypesFor(arrayOf(""), table))
        assertArrayEquals(arrayOf("*/*"), mimeTypesFor(emptyArray(), table))
    }

    @Test
    fun `a wildcard the picker understands is left alone`() {
        assertArrayEquals(arrayOf("image/*"), mimeTypesFor(arrayOf("image/*"), table))
    }
}
