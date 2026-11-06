package io.github.artcraftandroid.procraft.generation

import android.content.Context
import android.content.res.Configuration
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.artcraftandroid.procraft.canvas.CanvasViewModel
import io.github.artcraftandroid.procraft.core.Generation
import io.github.artcraftandroid.procraft.core.LoraSetting
import io.github.artcraftandroid.procraft.core.ServerLists
import io.github.artcraftandroid.procraft.core.StyleEdit
import io.github.artcraftandroid.procraft.ui.Pc
import io.github.artcraftandroid.procraft.ui.SliderRow
import kotlinx.coroutines.delay

/**
 * The Style Studio: a style's settings on a screen of their own, built like Brush Studio. The
 * pages by name on a card at the left (Model, LoRAs, Prompts, Sampling, Advanced), the open page's
 * settings beside it, and the preview on the right: Cancel and Done, the last try and the ones
 * before it, the preview prompt and Try it. Done writes the style's file in the styles folder
 * (what was written there by hand is kept); Cancel leaves it as it was, without asking.
 */
internal object StyleStudio {
    private class Open(val edit: StyleEdit, val file: String?, val oldText: String?)

    private var open by mutableStateOf<Open?>(null)
    private var edit by mutableStateOf<StyleEdit?>(null)
    private var page by mutableStateOf(0)
    private var picker by mutableStateOf<Picker?>(null)
    private var renaming by mutableStateOf(false)
    private var seed by mutableStateOf(0uL)
    private var error by mutableStateOf<String?>(null)
    // the tries, newest first: their number, picture and seed; and the one shown
    private val tries = mutableStateListOf<Triple<ULong, ImageBitmap, ULong>>()
    private var shown by mutableStateOf<ULong?>(null)
    // the preview prompt, kept app-wide
    private var prompt by mutableStateOf("a lighthouse on a cliff, scenery, no humans")
    // the saved style's file, for the app to choose it; the question about the folder
    var asking by mutableStateOf(false)

    private enum class Picker { SAMPLER, SCHEDULER, VAE, TEXT_ENCODER, LORA }
    private val Pages = listOf("Model", "LoRAs", "Prompts", "Sampling", "Advanced")

    private val Back get() = if (Pc.light) Color(0xFFF3F3F3) else Color(0xFF1F1F1F)
    private val Tabs get() = if (Pc.light) Color(0xFFEBEBEB) else Color(0xFF272727)
    private val Line get() = if (Pc.light) Color(0xFFCFCFCF) else Color(0xFF303030)
    private val Header get() = if (Pc.light) Color(0xFF818181) else Color(0xFF9C9C9C)
    private val CardEdge get() = if (Pc.light) Color(0xFFDADADA) else Color(0xFF373737)
    private val CardTitle get() = if (Pc.light) Color(0xFF4A4A4A) else Color(0xFFC1C1C1)
    private val PageInk get() = if (Pc.light) Color(0xFF3E3E3E) else Color(0xFF797979)
    private val ListFill get() = if (Pc.light) Color(0xFFFFFFFF) else Color(0xFF242424)
    private val ListLine get() = if (Pc.light) Color(0xFFE5E5E5) else Color(0xFF202020)
    private val Label get() = if (Pc.light) Color(0xFF343434) else Color(0xFFEDEDED)
    private val Dim get() = if (Pc.light) Color(0xFF9A9A9A) else Color(0xFF8E8E8E)
    private val Button get() = if (Pc.light) Color(0xFFFBFBFB) else Color(0xFF1C1C1C)
    private val ButtonEdge get() = if (Pc.light) Color(0xFFE0E0E0) else Color(0xFF383838)
    private val ButtonInk get() = if (Pc.light) Color(0xFF585858) else Color(0xFFAAAAAA)
    private val PreviewFill get() = if (Pc.light) Color(0xFFE8E8E8) else Color(0xFF2A2A2A)
    private val Warn = Color(0xFFF2B440)

    /** Opens the Studio on `edit`, kept in `file` (with its text `oldText`) when it's one of theirs. */
    fun start(g: Generation, edit: StyleEdit, file: String?, oldText: String?) {
        g.closeStudio()
        open = Open(edit, file, oldText)
        this.edit = edit
        page = 0
        picker = null
        renaming = false
        seed = (System.nanoTime() % 1_000_000_000L).toULong()
        error = null
        tries.clear()
        shown = null
    }

    val isOpen get() = open != null

    // the app's Generation, for the lists' choices
    private var gen: Generation? = null

    @Composable
    fun Screen(vm: CanvasViewModel, g: Generation, onClosed: (String?) -> Unit) {
        val context = LocalContext.current
        val o = open ?: return
        val e = edit ?: return
        gen = g
        val lists = remember(o) { g.serverLists() }
        var running by remember { mutableStateOf(false) }
        var progress by remember { mutableStateOf(0f) }
        var waiting by remember { mutableStateOf(false) }
        // Try it, looked at while the Studio is open
        LaunchedEffect(o) {
            while (true) {
                val s = g.studioState()
                running = s.running
                progress = s.progress
                waiting = s.waiting
                s.picture?.let { p ->
                    if (tries.none { it.first == p.id }) {
                        tries.add(0, Triple(p.id, p.picture.toBitmap(), p.seed))
                        while (tries.size > 4) tries.removeAt(tries.size - 1)
                        shown = p.id
                    }
                }
                s.error?.let { error = it }
                delay(100)
            }
        }
        fun close(saved: String?) {
            g.closeStudio()
            open = null
            edit = null
            onClosed(saved)
        }
        fun done() {
            val name = e.name.trim().ifEmpty { "Style" }
            val file = g.styleFileName(name, o.file)
            val text = g.styleText(e.copy(name = name), o.oldText)
            val problem = StylesFolder.write(context, file, text, o.file)
            if (problem != null) {
                error = "Couldn't save the style: $problem"
                return
            }
            // the trigger words, kept on the server for every style
            g.saveLoraNotes(e.loras)
            // the first save without a folder given: ask for it once the Studio's gone
            if (!StylesFolder.granted(context)) asking = true
            close(file)
        }
        val landscape = LocalConfiguration.current.orientation == Configuration.ORIENTATION_LANDSCAPE
        Box(Modifier.fillMaxSize().background(Back).pointerInput(Unit) { awaitEachGesture { awaitFirstDown().consume() } }) {
            if (landscape) {
                // above the keyboard while it's up, so what's typed stays in view
                Row(Modifier.fillMaxSize().imePadding()) {
                    PagesCard()
                    Settings(e, lists, g, Modifier.width(323.dp).fillMaxHeight())
                    Box(Modifier.width(1.dp).fillMaxHeight().background(Line))
                    Preview(vm, g, e, running, progress, waiting, Modifier.fillMaxSize(), { close(null) }, { done() })
                }
            } else {
                Column(Modifier.fillMaxSize().imePadding()) {
                    Preview(vm, g, e, running, progress, waiting, Modifier.fillMaxWidth().height(510.dp), { close(null) }, { done() })
                    Box(Modifier.fillMaxWidth().height(1.dp).background(Line))
                    Row(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        Pages.forEachIndexed { i, name ->
                            val on = page == i
                            Box(Modifier.height(36.dp).clip(RoundedCornerShape(18.dp)).background(if (on) Pc.Accent else Color.Transparent).tap { page = i; picker = null }.padding(horizontal = 12.dp), contentAlignment = Alignment.Center) {
                                Text(name, color = if (on) Color.White else PageInk, fontSize = 14.sp)
                            }
                        }
                    }
                    Settings(e, lists, g, Modifier.fillMaxSize())
                }
            }
        }
    }

    @Composable
    private fun PagesCard() {
        val shape = RoundedCornerShape(20.dp)
        Column(Modifier.padding(start = 10.dp, top = 10.dp, bottom = 10.dp).width(320.dp).fillMaxHeight().clip(shape).background(Tabs).border(1.dp, CardEdge, shape)) {
            Text("Style Studio", color = CardTitle, fontSize = 23.sp, fontWeight = FontWeight.Bold, maxLines = 1, modifier = Modifier.padding(start = 24.dp, top = 19.dp))
            Column(Modifier.padding(start = 8.dp, end = 8.dp, top = 17.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Pages.forEachIndexed { i, name ->
                    val on = page == i
                    Row(Modifier.fillMaxWidth().height(44.dp).clip(RoundedCornerShape(22.dp)).background(if (on) Pc.Accent else Color.Transparent).tap { page = i; picker = null }, verticalAlignment = Alignment.CenterVertically) {
                        Text(name, color = if (on) Color.White else PageInk, fontSize = 15.5.sp, maxLines = 1, modifier = Modifier.padding(start = 20.dp))
                    }
                }
            }
        }
    }

    @Composable
    private fun Settings(e: StyleEdit, lists: ServerLists, g: Generation, modifier: Modifier) {
        Column(modifier.verticalScroll(remember(page, picker) { androidx.compose.foundation.ScrollState(0) }).padding(start = 20.dp, end = 20.dp, top = 25.dp, bottom = 77.dp)) {
            val p = picker
            if (p != null) {
                PickerList(p, e, lists)
                return@Column
            }
            when (page) {
                0 -> ModelPage(e, lists, g)
                1 -> LorasPage(e, lists)
                2 -> PromptsPage(e)
                3 -> SamplingPage(e)
                else -> AdvancedPage(e)
            }
        }
    }

    @Composable
    private fun ColumnScope.ModelPage(e: StyleEdit, lists: ServerLists, g: Generation) {
        // the name heads the page, tapped to change it there, as Brush Studio's About page has it
        if (renaming) {
            val focus = remember { FocusRequester() }
            var had by remember { mutableStateOf(false) }
            Field(e.name, false, Modifier.focusRequester(focus).onFocusChanged { if (it.isFocused) had = true else if (had) renaming = false }) { edit = e.copy(name = it) }
            LaunchedEffect(Unit) { focus.requestFocus() }
        } else {
            Text(e.name.ifBlank { "Untitled style" }, color = Header, fontSize = 21.sp, fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(start = 8.dp).tap { renaming = true })
        }
        Spacer(Modifier.height(18.dp))
        Heading("Model")
        val chosen = remember(e.models, lists) { g.checkpointFor(e.models) }
        if (lists.checkpoints.isEmpty()) Note("Not connected to a server")
        val rows = buildList {
            if (chosen == null) add((e.models.firstOrNull() ?: "") + " — not on this server" to true)
            lists.checkpoints.forEach { add(it to (it == chosen)) }
        }
        ChoiceList(rows) { i ->
            val index = if (chosen == null) i - 1 else i
            lists.checkpoints.getOrNull(index)?.let { edit = e.copy(models = listOf(it)) }
        }
    }

    @Composable
    private fun LorasPage(e: StyleEdit, lists: ServerLists) {
        Heading("LoRAs")
        if (e.loras.isEmpty()) Note("None yet. A LoRA pushes a look or a character; add one below.")
        e.loras.forEachIndexed { i, lora ->
            val missing = lists.loras.isNotEmpty() && lora.file !in lists.loras
            Column(Modifier.fillMaxWidth().padding(bottom = 8.dp).clip(RoundedCornerShape(14.dp)).background(ListFill)) {
                Row(Modifier.fillMaxWidth().padding(start = 14.dp, top = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text(loraName(lora.file), color = if (missing) Warn else Label, fontSize = 15.sp, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        if (missing) Text("not on this server", color = Warn, fontSize = 11.sp)
                    }
                    Text("×", color = Dim, fontSize = 22.sp, modifier = Modifier.tap { edit = e.copy(loras = e.loras.filterIndexed { j, _ -> j != i }) }.padding(horizontal = 14.dp))
                }
                SliderRow("Strength", lora.strength / 2f, "${Math.round(lora.strength * 100)} %") { t ->
                    val strength = Math.round(t * 40) / 20f
                    edit = e.copy(loras = e.loras.mapIndexed { j, l -> if (j == i) l.copy(strength = strength) else l })
                }
                // its trigger words, kept on the server for every style that uses it
                Column(Modifier.padding(start = 14.dp, end = 14.dp, bottom = 12.dp)) {
                    Text("Trigger words", color = Dim, fontSize = 13.sp, modifier = Modifier.padding(bottom = 6.dp))
                    Field(lora.trigger, false) { words -> edit = e.copy(loras = e.loras.mapIndexed { j, l -> if (j == i) l.copy(trigger = words) else l }) }
                    Text("Used by every style with this LoRA, added after your prompt.", color = Dim, fontSize = 12.sp, modifier = Modifier.padding(top = 6.dp))
                }
            }
        }
        ButtonRow("+ Add LoRA") { picker = Picker.LORA }
    }

    @Composable
    private fun PromptsPage(e: StyleEdit) {
        Heading("Prompt")
        Field(e.template, true) { edit = e.copy(template = it) }
        Note(if ("{prompt}" in e.template) "{prompt} is where your prompt goes" else "Your prompt goes at the end")
        Spacer(Modifier.height(14.dp))
        Heading("Negative prompt")
        Field(e.negative, true) { edit = e.copy(negative = it) }
    }

    @Composable
    private fun SamplingPage(e: StyleEdit) {
        Heading("Sampling")
        ValueRow("Sampler", e.sampler) { picker = Picker.SAMPLER }
        Spacer(Modifier.height(10.dp))
        SliderRow("Steps", (e.steps.toFloat() - 1f) / 99f, "${e.steps}") { t -> edit = e.copy(steps = (1 + Math.round(t * 99)).toUInt()) }
        SliderRow("CFG", (e.cfg - 1f) / 14f, "%.1f".format(e.cfg)) { t -> edit = e.copy(cfg = Math.round((1f + t * 14f) * 10f) / 10f) }
    }

    @Composable
    private fun AdvancedPage(e: StyleEdit) {
        Heading("Advanced")
        ValueRow("Scheduler", e.scheduler) { picker = Picker.SCHEDULER }
        Spacer(Modifier.height(10.dp))
        SwitchRow("Rescale CFG", e.rescaleCfg != null) { edit = e.copy(rescaleCfg = if (e.rescaleCfg != null) null else 0.7f) }
        e.rescaleCfg?.let { r -> SliderRow("Multiplier", r, "%.2f".format(r)) { t -> edit = e.copy(rescaleCfg = Math.round(t * 100) / 100f) } }
        Spacer(Modifier.height(10.dp))
        ValueRow("VAE", e.vae ?: "The model's own") { picker = Picker.VAE }
        Spacer(Modifier.height(10.dp))
        ValueRow("Text encoder", e.textEncoder ?: "The model's own") { picker = Picker.TEXT_ENCODER }
        Spacer(Modifier.height(10.dp))
        SwitchRow("Best size", e.bestSize != null) { edit = e.copy(bestSize = if (e.bestSize != null) null else 1024u) }
        e.bestSize?.let { size ->
            SliderRow("One side", (size.toFloat() - 256f) / (2048f - 256f), "$size px") { t -> edit = e.copy(bestSize = (Math.round((256f + t * (2048f - 256f)) / 8f) * 8).toUInt()) }
            Note("Pictures are made at about this size squared, then scaled. For models trained at an unusual size.")
        }
    }

    @Composable
    private fun PickerList(p: Picker, e: StyleEdit, lists: ServerLists) {
        val (title, options, current) = when (p) {
            Picker.SAMPLER -> Triple("Sampler", lists.samplers, e.sampler)
            Picker.SCHEDULER -> Triple("Scheduler", lists.schedulers, e.scheduler)
            Picker.VAE -> Triple("VAE", listOf("") + lists.vaes, e.vae ?: "")
            Picker.TEXT_ENCODER -> Triple("Text encoder", listOf("") + lists.textEncoders, e.textEncoder ?: "")
            Picker.LORA -> Triple("Add a LoRA", lists.loras.filter { f -> e.loras.none { it.file == f } }, null)
        }
        Text("‹ ${Pages[page]}", color = Pc.Accent, fontSize = 15.sp, modifier = Modifier.padding(start = 8.dp, bottom = 6.dp).tap { picker = null })
        Heading(title)
        if (options.isEmpty()) {
            Note(if (lists.checkpoints.isEmpty()) "Not connected to a server" else "The server has none")
            return
        }
        ChoiceList(options.map { (if (it.isEmpty()) "The model's own" else if (p == Picker.LORA) loraName(it) else it) to (it == current) }) { i ->
            val choice = options.getOrNull(i) ?: return@ChoiceList
            edit = when (p) {
                Picker.SAMPLER -> e.copy(sampler = choice)
                Picker.SCHEDULER -> e.copy(scheduler = choice)
                Picker.VAE -> e.copy(vae = choice.ifEmpty { null })
                Picker.TEXT_ENCODER -> e.copy(textEncoder = choice.ifEmpty { null })
                Picker.LORA -> e.copy(loras = e.loras + LoraSetting(choice, 1f, gen?.loraTrigger(choice) ?: ""))
            }
            picker = null
        }
    }

    @Composable
    private fun Preview(vm: CanvasViewModel, g: Generation, e: StyleEdit, running: Boolean, progress: Float, waiting: Boolean, modifier: Modifier, onCancel: () -> Unit, onDone: () -> Unit) {
        val info = vm.session?.info()
        val ratio = info?.let { it.width.toFloat() / it.height.toFloat().coerceAtLeast(1f) } ?: 1f
        Column(modifier.padding(start = 30.dp, end = 20.dp, bottom = 20.dp)) {
            Row(Modifier.fillMaxWidth().padding(top = 20.dp), horizontalArrangement = Arrangement.End, verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.size(73.dp, 43.dp).clip(CircleShape).background(Button).border(1.dp, ButtonEdge, CircleShape).tap(onClick = onCancel), contentAlignment = Alignment.Center) {
                    Text("Cancel", color = ButtonInk, fontSize = 17.sp)
                }
                Spacer(Modifier.width(13.dp))
                Box(Modifier.size(44.dp).clip(CircleShape).background(Color(0xFF0190FF)).tap(onClick = onDone), contentAlignment = Alignment.Center) {
                    Text("✓", color = Color.White, fontSize = 22.sp, fontWeight = FontWeight.Bold)
                }
            }
            // the try shown, in the canvas's shape
            Box(Modifier.fillMaxWidth().weight(1f).padding(vertical = 14.dp), contentAlignment = Alignment.Center) {
                Box(Modifier.aspectRatio(ratio.coerceIn(0.2f, 5f)).clip(RoundedCornerShape(12.dp)).background(PreviewFill), contentAlignment = Alignment.Center) {
                    val picture = tries.firstOrNull { it.first == shown }?.second
                    if (picture != null) Image(picture, contentDescription = "The last try", modifier = Modifier.fillMaxSize())
                    else if (!running) Text("Try it shows the style as it is", color = Dim, fontSize = 15.sp)
                    if (running) Column(horizontalAlignment = Alignment.CenterHorizontally) {
                        androidx.compose.material3.CircularProgressIndicator(progress = { progress }, color = Pc.Accent, modifier = Modifier.size(44.dp))
                        if (waiting) Text("Waiting for the canvas's run", color = Dim, fontSize = 13.sp, modifier = Modifier.padding(top = 10.dp))
                    }
                }
            }
            // the tries, newest on the left
            if (tries.isNotEmpty()) Row(Modifier.padding(bottom = 10.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                for ((id, picture, _) in tries) {
                    Image(picture, contentDescription = null, modifier = Modifier.size(56.dp).clip(RoundedCornerShape(6.dp)).then(if (id == shown) Modifier.border(2.dp, Pc.Accent, RoundedCornerShape(6.dp)) else Modifier).tap { shown = id })
                }
            }
            Box(Modifier.fillMaxWidth().height(44.dp).clip(RoundedCornerShape(12.dp)).background(PreviewFill).padding(horizontal = 12.dp), contentAlignment = Alignment.CenterStart) {
                BasicTextField(prompt, { prompt = it }, singleLine = true, textStyle = TextStyle(color = Label, fontSize = 14.sp), cursorBrush = SolidColor(Pc.Accent), modifier = Modifier.fillMaxWidth())
            }
            Row(Modifier.padding(top = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.size(40.dp).clip(CircleShape).background(Button).tap { seed = (System.nanoTime() % 1_000_000_000L).toULong() }, contentAlignment = Alignment.Center) {
                    Text("⚄", color = ButtonInk, fontSize = 20.sp)
                }
                Spacer(Modifier.width(12.dp))
                val words = if (running) "Cancel · ${Math.round(progress * 100)}%" else "Try it"
                Box(Modifier.height(38.dp).clip(RoundedCornerShape(19.dp)).background(if (running) Color(0xFF5A5A5A) else Pc.Accent).tap {
                    if (running) g.cancelTry() else {
                        error = null
                        val w = info?.width ?: 1024u
                        val h = info?.height ?: 1024u
                        try { g.tryStyle(e, prompt, seed, w, h) } catch (x: Exception) { error = x.message }
                    }
                }.padding(horizontal = 20.dp), contentAlignment = Alignment.Center) {
                    Text(words, color = Color.White, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
                }
                Spacer(Modifier.width(14.dp))
                Text("Seed $seed", color = Dim, fontSize = 12.sp)
            }
            error?.let { Text(it, color = Warn, fontSize = 12.5.sp, maxLines = 2, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(top = 8.dp)) }
        }
    }

    // pieces

    @Composable
    private fun Heading(words: String) {
        Text(words, color = Header, fontSize = 21.sp, fontWeight = FontWeight.Bold, maxLines = 1, modifier = Modifier.padding(start = 8.dp, bottom = 8.dp))
    }

    @Composable
    private fun Note(words: String) {
        Text(words, color = Dim, fontSize = 12.5.sp, modifier = Modifier.padding(start = 8.dp, top = 4.dp, bottom = 8.dp))
    }

    @Composable
    private fun Field(value: String, many: Boolean, modifier: Modifier = Modifier, onChange: (String) -> Unit) {
        Box(Modifier.fillMaxWidth().heightIn(min = if (many) 110.dp else 44.dp).clip(RoundedCornerShape(12.dp)).background(PreviewFill).padding(horizontal = 12.dp, vertical = 11.dp)) {
            BasicTextField(value, onChange, singleLine = !many, textStyle = TextStyle(color = Label, fontSize = 14.sp), cursorBrush = SolidColor(Pc.Accent), modifier = modifier.fillMaxWidth())
        }
    }

    @Composable
    private fun ValueRow(name: String, value: String, onTap: () -> Unit) {
        Row(Modifier.fillMaxWidth().height(46.dp).clip(RoundedCornerShape(14.dp)).background(ListFill).tap(onClick = onTap).padding(horizontal = 14.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(name, color = Label, fontSize = 15.sp, modifier = Modifier.weight(1f))
            Text(value, color = Dim, fontSize = 13.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false))
            Text(" ›", color = Dim, fontSize = 17.sp)
        }
    }

    @Composable
    private fun SwitchRow(name: String, on: Boolean, onTap: () -> Unit) {
        Row(Modifier.fillMaxWidth().height(46.dp).clip(RoundedCornerShape(14.dp)).background(ListFill).tap(onClick = onTap).padding(horizontal = 14.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(name, color = Label, fontSize = 15.sp, modifier = Modifier.weight(1f))
            Text("Override", color = Dim, fontSize = 12.sp, modifier = Modifier.padding(end = 10.dp))
            Box(Modifier.size(44.dp, 26.dp).clip(RoundedCornerShape(13.dp)).background(if (on) Pc.Accent else Dim.copy(alpha = 0.4f)).padding(3.dp), contentAlignment = if (on) Alignment.CenterEnd else Alignment.CenterStart) {
                Box(Modifier.size(20.dp).clip(CircleShape).background(Color.White))
            }
        }
    }

    @Composable
    private fun ButtonRow(words: String, onTap: () -> Unit) {
        Box(Modifier.fillMaxWidth().height(44.dp).clip(RoundedCornerShape(14.dp)).background(ListFill).tap(onClick = onTap), contentAlignment = Alignment.Center) {
            Text(words, color = Pc.Accent, fontSize = 15.sp, fontWeight = FontWeight.Medium)
        }
    }

    @Composable
    private fun ChoiceList(rows: List<Pair<String, Boolean>>, onPick: (Int) -> Unit) {
        Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).background(ListFill)) {
            rows.forEachIndexed { i, (name, on) ->
                if (i > 0) Box(Modifier.fillMaxWidth().padding(start = 14.dp).height(1.dp).background(ListLine))
                Row(Modifier.fillMaxWidth().height(44.dp).tap { onPick(i) }.padding(horizontal = 14.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(name, color = if (name.endsWith("not on this server")) Warn else Label, fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                    if (on) Text("✓", color = Pc.Accent, fontSize = 16.sp, fontWeight = FontWeight.Bold)
                }
            }
        }
    }

    private fun loraName(file: String) = file.substringAfterLast('/').substringAfterLast('\\').removeSuffix(".safetensors").replace('_', ' ')

    private fun io.github.artcraftandroid.procraft.core.ThumbnailImage.toBitmap(): ImageBitmap {
        val px = IntArray((width * height).toInt())
        val b = rgba
        for (i in px.indices) {
            val o = i * 4
            px[i] = ((b[o + 3].toInt() and 0xFF) shl 24) or ((b[o].toInt() and 0xFF) shl 16) or ((b[o + 1].toInt() and 0xFF) shl 8) or (b[o + 2].toInt() and 0xFF)
        }
        val bmp = android.graphics.Bitmap.createBitmap(px, width.toInt(), height.toInt(), android.graphics.Bitmap.Config.ARGB_8888)
        bmp.isPremultiplied = true
        return bmp.asImageBitmap()
    }

    private fun Modifier.tap(onClick: () -> Unit): Modifier =
        this.clickable(interactionSource = null, indication = null, onClick = onClick)
}

@Composable
private fun Text(text: String, color: Color, fontSize: androidx.compose.ui.unit.TextUnit, modifier: Modifier = Modifier, fontWeight: FontWeight? = null, maxLines: Int = Int.MAX_VALUE, overflow: TextOverflow = TextOverflow.Clip) =
    androidx.compose.material3.Text(text, color = color, fontSize = fontSize, modifier = modifier, fontWeight = fontWeight, maxLines = maxLines, overflow = overflow)
