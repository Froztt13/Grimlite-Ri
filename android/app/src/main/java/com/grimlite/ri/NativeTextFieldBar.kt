package com.grimlite.ri

import android.content.Context
import android.graphics.Color
import android.graphics.Rect
import android.graphics.drawable.GradientDrawable
import android.os.Build
import android.text.Editable
import android.text.InputFilter
import android.text.InputType
import android.text.TextWatcher
import android.text.method.PasswordTransformationMethod
import android.util.TypedValue
import android.view.Gravity
import android.view.KeyEvent
import android.view.View
import android.view.ViewGroup
import android.view.WindowInsets
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputMethodManager
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView
import androidx.core.view.WindowInsetsCompat

/**
 * Native Android text input strip docked directly above the soft keyboard (matching Turu's
 * `NativeField` + `SurfaceField` architecture).
 *
 * Attached to `android.R.id.content` at `Gravity.BOTTOM` over `GameActivity`'s `SurfaceView` with
 * bottom padding driven by `WindowInsetsCompat.Type.ime()`, so it sits flush on top of the
 * keyboard without shifting or shrinking the Flash viewport (`adjustNothing`).
 */
internal class NativeTextFieldBar(
    private val activity: MainActivity,
    private val target: FieldTarget,
) {
    interface FieldTarget {
        fun edited(text: String, start: Int, end: Int)
        fun enter()
        fun closed()
    }

    private var bar: LinearLayout? = null
    private var edit: Field? = null
    private var count: TextView? = null
    private var max: Int = 0
    private var fromOwner: Boolean = false
    private var keyboardUp: Boolean = false

    val shown: Boolean
        get() = bar != null

    fun show(text: String, start: Int, end: Int, password: Boolean, maxChars: Int, hint: String) {
        hide(false)
        keyboardUp = false
        max = maxChars

        val field = Field(activity, this)
        field.isSingleLine = true
        field.inputType = if (password) {
            InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD
        } else {
            InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_AUTO_CORRECT
        }
        if (password) {
            field.transformationMethod = PasswordTransformationMethod.getInstance()
        }
        field.imeOptions =
            EditorInfo.IME_ACTION_SEND or
                EditorInfo.IME_FLAG_NO_EXTRACT_UI or
                EditorInfo.IME_FLAG_NO_FULLSCREEN
        if (maxChars > 0) {
            field.filters = arrayOf(InputFilter.LengthFilter(maxChars))
        }
        field.setTextSize(TypedValue.COMPLEX_UNIT_SP, 18f)
        field.setTextColor(TEXT)
        field.setHintTextColor(MUTED)
        field.hint = hint
        field.highlightColor = (ACCENT and 0x00FFFFFF) or 0x66000000
        field.background = null
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            field.textCursorDrawable = GradientDrawable().apply {
                setColor(ACCENT)
                setSize(dp(2), 0)
            }
            field.textSelectHandle?.setTint(ACCENT)
            field.textSelectHandleLeft?.setTint(ACCENT)
            field.textSelectHandleRight?.setTint(ACCENT)
        }
        field.setOnEditorActionListener { _, editorAction, event ->
            val enter =
                event?.keyCode == KeyEvent.KEYCODE_ENTER && event.action == KeyEvent.ACTION_DOWN
            val send =
                editorAction == EditorInfo.IME_ACTION_SEND ||
                    editorAction == EditorInfo.IME_ACTION_DONE
            if (send || enter) {
                target.enter()
                true
            } else {
                false
            }
        }

        val countView = TextView(activity).apply {
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 13f)
            setTextColor(MUTED)
            setPadding(dp(12), 0, 0, 0)
            visibility = if (maxChars > 0) View.VISIBLE else View.GONE
        }

        val container = LinearLayout(activity).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            isClickable = true
            setBackgroundColor(CARD)
            setPadding(dp(16), dp(4), dp(16), dp(4))
            addView(field, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
            addView(
                countView,
                LinearLayout.LayoutParams(
                    ViewGroup.LayoutParams.WRAP_CONTENT,
                    ViewGroup.LayoutParams.WRAP_CONTENT,
                ),
            )
        }

        bar = container
        edit = field
        count = countView
        set(text, start, end)

        field.addTextChangedListener(object : TextWatcher {
            override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
            override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {}
            override fun afterTextChanged(s: Editable?) {
                showCount()
                edited()
            }
        })

        // Keep strip flush above the soft keyboard and clear of side display cutouts in landscape.
        container.setOnApplyWindowInsetsListener { view, insets ->
            val kbHeight = keyboardHeight(insets)
            keyboardMoved(kbHeight)
            var cutoutLeft = 0
            var cutoutRight = 0
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
                cutoutLeft = insets.displayCutout?.safeInsetLeft ?: 0
                cutoutRight = insets.displayCutout?.safeInsetRight ?: 0
            }
            view.setPadding(dp(16) + cutoutLeft, dp(4), dp(16) + cutoutRight, dp(4) + kbHeight)
            insets
        }

        val content = activity.findViewById<ViewGroup>(android.R.id.content)
        val lp = FrameLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT,
            ViewGroup.LayoutParams.WRAP_CONTENT,
            Gravity.BOTTOM,
        )
        content.addView(container, lp)
        container.requestApplyInsets()

        field.requestFocus()
        field.post {
            edit?.let { set(it.text.toString(), it.selectionStart, it.selectionEnd) }
            field.scrollTo(0, 0)
            field.bringPointIntoView(field.selectionEnd)
            val imm =
                activity.getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager
            imm.showSoftInput(field, InputMethodManager.SHOW_IMPLICIT)
        }
    }

    fun update(text: String, start: Int, end: Int) {
        val field = edit ?: return
        if (field.text.toString() == text &&
            field.selectionStart == start &&
            field.selectionEnd == end
        ) {
            return
        }
        set(text, start, end)
    }

    fun hide(close: Boolean = false) {
        val container = bar ?: return
        bar = null
        val field = edit
        edit = null
        count = null
        val imm = activity.getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager
        imm.hideSoftInputFromWindow(container.windowToken, 0)
        field?.clearFocus()
        (container.parent as? ViewGroup)?.removeView(container)
        activity.focusSurface()
        if (close) {
            target.closed()
        }
    }

    fun close() = hide(true)

    fun touchedOutside(x: Float, y: Float): Boolean {
        val container = bar ?: return false
        if (!container.isLaidOut) return false
        return !visibleBarArea(container).contains(x.toInt(), y.toInt())
    }

    private fun visibleBarArea(container: LinearLayout): Rect {
        val rect = Rect()
        container.getGlobalVisibleRect(rect)
        // Exclude bottom keyboard padding so taps below the strip (if any) aren't treated as inside.
        rect.bottom = (rect.bottom - container.paddingBottom).coerceAtLeast(rect.top)
        return rect
    }

    private fun set(text: String, start: Int, end: Int) {
        val field = edit ?: return
        fromOwner = true
        try {
            if (field.text.toString() != text) {
                field.setText(text)
            }
            val len = field.text.length
            field.setSelection(start.coerceIn(0, len), end.coerceIn(0, len))
        } finally {
            fromOwner = false
        }
        showCount()
    }

    private fun showCount() {
        val field = edit ?: return
        if (max > 0) {
            count?.text = "${field.text.length}/$max"
            count?.setTextColor(if (field.text.length >= max) DANGER else MUTED)
        }
    }

    private fun edited() {
        val field = edit ?: return
        if (fromOwner) return
        target.edited(field.text.toString(), field.selectionStart, field.selectionEnd)
    }

    private fun keyboardMoved(height: Int) {
        val up = height > 0
        if (keyboardUp && !up && bar != null) {
            activity.window.decorView.post {
                if (bar != null) close()
            }
        }
        keyboardUp = up
    }

    private fun dp(value: Int): Int = (value * activity.resources.displayMetrics.density).toInt()

    private class Field(context: Context, private val owner: NativeTextFieldBar) :
        EditText(context) {
        override fun onKeyPreIme(keyCode: Int, event: KeyEvent): Boolean {
            if (keyCode == KeyEvent.KEYCODE_BACK && event.action == KeyEvent.ACTION_UP) {
                owner.close()
                return true
            }
            return super.onKeyPreIme(keyCode, event)
        }

        override fun onSelectionChanged(selStart: Int, selEnd: Int) {
            super.onSelectionChanged(selStart, selEnd)
            @Suppress("SENSELESS_COMPARISON")
            if (owner != null) {
                owner.edited()
            }
        }
    }

    companion object {
        private val CARD = Color.rgb(24, 27, 37)
        private val TEXT = Color.rgb(232, 234, 242)
        private val MUTED = Color.rgb(140, 146, 166)
        private val DANGER = Color.rgb(0xEF, 0x44, 0x44)
        private val ACCENT = Color.rgb(0xE6, 0x39, 0x46)

        private fun keyboardHeight(insets: WindowInsets): Int {
            val compat = WindowInsetsCompat.toWindowInsetsCompat(insets)
            val ime = WindowInsetsCompat.Type.ime()
            return if (compat.isVisible(ime)) compat.getInsets(ime).bottom else 0
        }
    }
}
