// Mono recolor: turns WhatsApp Web's colors into Wisp's graphite palette with a
// single accent. WhatsApp's class and variable names are generated and change
// between releases, so this works on color *values* instead of names:
//
// - vivid greens, teals and blues (WhatsApp's brand and action colors) take the
//   accent's hue at their original lightness, so WhatsApp's own contrast holds;
// - other greens and decorative pinks and purples become gray at the same lightness;
// - reds, oranges and yellows stay, because they mean errors and warnings;
// - the chat wallpaper's gradient and doodle image are dropped for a flat page.
//
// It reads every stylesheet rule that sets a color and emits a matching rule
// with a more specific selector (scoped under html.wisp-themed), so turning the
// theme off simply removes the generated stylesheet.
const wispMono = (() => {
  const parse = (s) => {
    let m = /^#([0-9a-f]{3,8})$/i.exec(s);
    if (m) {
      let h = m[1];
      if (h.length === 3 || h.length === 4) h = [...h].map((c) => c + c).join("");
      if (h.length !== 6 && h.length !== 8) return null;
      const n = (i) => parseInt(h.slice(i, i + 2), 16);
      return { r: n(0), g: n(2), b: n(4), a: h.length === 8 ? n(6) / 255 : 1 };
    }
    m = /^rgba?\(\s*([\d.]+)[\s,]+([\d.]+)[\s,]+([\d.]+)(?:\s*[,/]\s*([\d.]+%?))?\s*\)$/i.exec(s);
    if (!m) return null;
    let a = m[4] === undefined ? 1 : m[4].endsWith("%") ? parseFloat(m[4]) / 100 : parseFloat(m[4]);
    return { r: +m[1], g: +m[2], b: +m[3], a };
  };

  const toHsl = ({ r, g, b }) => {
    r /= 255; g /= 255; b /= 255;
    const max = Math.max(r, g, b), min = Math.min(r, g, b), l = (max + min) / 2, d = max - min;
    if (d === 0) return { h: 0, s: 0, l };
    const s = d / (1 - Math.abs(2 * l - 1));
    let h = max === r ? ((g - b) / d) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
    return { h: (h * 60 + 360) % 360, s, l };
  };

  const fromHsl = ({ h, s, l }) => {
    const c = (1 - Math.abs(2 * l - 1)) * s, x = c * (1 - Math.abs(((h / 60) % 2) - 1)), m = l - c / 2;
    const [r, g, b] = h < 60 ? [c, x, 0] : h < 120 ? [x, c, 0] : h < 180 ? [0, c, x] : h < 240 ? [0, x, c] : h < 300 ? [x, 0, c] : [c, 0, x];
    return { r: Math.round((r + m) * 255), g: Math.round((g + m) * 255), b: Math.round((b + m) * 255) };
  };

  const format = ({ r, g, b }, a) => (a >= 1 ? `rgb(${r}, ${g}, ${b})` : `rgba(${r}, ${g}, ${b}, ${+a.toFixed(3)})`);

  /** The mono version of one color, or null to leave it as it is. */
  const recolor = (value, accent) => {
    const c = parse(value.trim());
    if (!c) return null;
    const { h, s, l } = toHsl(c);
    const chroma = (Math.max(c.r, c.g, c.b) - Math.min(c.r, c.g, c.b)) / 255;
    if (chroma < 0.06 || s < 0.12) return null; // already neutral
    if (h < 65 || h >= 345) return null; // red, orange, yellow: errors and warnings
    const brand = h >= 70 && h <= 230;
    if (brand && s >= 0.4 && l >= 0.22 && l <= 0.82) {
      return format(fromHsl({ h: accent.h, s: Math.min(s, Math.max(accent.s, 0.55)), l }), c.a);
    }
    const g = Math.round(l * 255);
    return format({ r: g, g, b: g }, c.a);
  };

  const COLOR = /#[0-9a-fA-F]{3,8}\b|rgba?\([^()]*\)/g;
  const recolorValue = (value, accent) => {
    let changed = false;
    const out = value.replace(COLOR, (m) => {
      const r = recolor(m, accent);
      if (r === null) return m;
      changed = true;
      return r;
    });
    return changed ? out : null;
  };

  // Split a selector list on top-level commas (not inside :is(), :not() ...).
  const splitSelectors = (text) => {
    const parts = [];
    let depth = 0, start = 0;
    for (let i = 0; i < text.length; i++) {
      const ch = text[i];
      if (ch === "(" || ch === "[") depth++;
      else if (ch === ")" || ch === "]") depth--;
      else if (ch === "," && depth === 0) { parts.push(text.slice(start, i)); start = i + 1; }
    }
    parts.push(text.slice(start));
    return parts.map((p) => p.trim()).filter(Boolean);
  };

  const scope = (selector) =>
    splitSelectors(selector)
      .map((s) => {
        if (s.includes(":root")) return s.replace(/:root/g, ":root.wisp-themed");
        if (/^html(?![\w-])/.test(s)) return s.replace(/^html/, "html.wisp-themed");
        return "html.wisp-themed " + s;
      })
      .join(", ");

  const ruleCss = (rule, accent) => {
    const decls = [];
    const style = rule.style;
    const wallpaper = /data-asset-chat-background/.test(rule.selectorText);
    for (let i = 0; i < style.length; i++) {
      const prop = style[i];
      const value = style.getPropertyValue(prop);
      if (!value) continue;
      const important = style.getPropertyPriority(prop) === "important" ? " !important" : "";
      // The wallpaper's gradient is painted with this token; flat means none.
      if (/chat-foreground-wallpaper$/.test(prop)) {
        decls.push(`${prop}: transparent${important}`);
        continue;
      }
      if (wallpaper && prop === "background-image") {
        decls.push("background-image: none !important");
        continue;
      }
      const next = recolorValue(value, accent);
      if (next !== null) decls.push(`${prop}: ${next}${important}`);
    }
    return decls.length ? `${scope(rule.selectorText)} { ${decls.join("; ")} }` : "";
  };

  const walk = (rules, accent, out) => {
    for (const rule of rules) {
      if (rule.selectorText !== undefined && rule.style) {
        const css = ruleCss(rule, accent);
        if (css) out.push(css);
      } else if (rule.cssRules) {
        const inner = [];
        walk(rule.cssRules, accent, inner);
        if (!inner.length) continue;
        if (rule.media) out.push(`@media ${rule.media.mediaText} { ${inner.join("\n")} }`);
        else if (rule.conditionText !== undefined) out.push(`@supports ${rule.conditionText} { ${inner.join("\n")} }`);
        else out.push(...inner); // e.g. @layer: unlayered rules win anyway
      }
    }
  };

  /** CSS that recolors every readable stylesheet except Wisp's own. */
  const build = (accentColor) => {
    const parsed = parse(accentColor) || { r: 6, g: 188, b: 188, a: 1 };
    const accent = toHsl(parsed);
    const out = [];
    for (const sheet of document.styleSheets) {
      const owner = sheet.ownerNode;
      if (owner && owner.id && owner.id.startsWith("wisp-")) continue;
      let rules;
      try { rules = sheet.cssRules; } catch (e) { continue; }
      if (rules) walk(rules, accent, out);
    }
    return out.join("\n");
  };

  return { build, recolor: (v, a) => recolor(v, toHsl(parse(a))), scope, splitSelectors };
})();
