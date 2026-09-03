const DATA = window.__PARITY__;
const STATES = [
  ["same","same","●"],["reshaped","re-shaped","●"],["renamed","renamed","●"],
  ["remapped","re-mapped","◆"],["rejected","rejected","✕"],
  ["missing","not implemented","○"],["ours","ours only","✥"]
];
const GLYPH = Object.fromEntries(STATES.map(s => [s[0], s[2]]));
const LABEL = Object.fromEntries(STATES.map(s => [s[0], s[1]]));
const on = new Set(STATES.map(s => s[0]));
let query = "";

const esc = s => String(s == null ? "" : s)
  .replace(/&/g,"&amp;").replace(/</g,"&lt;").replace(/>/g,"&gt;");
// ledger prose is written with `backticks`; render them as code, escaping first
const prose = s => esc(s).replace(/`([^`]+)`/g, (_, c) => "<code>" + c + "</code>");
const tag = s => '<span class="tag ' + s + '"><span>' + GLYPH[s] + "</span>" + LABEL[s] + "</span>";

function buildChips() {
  const host = document.getElementById("chips");
  host.innerHTML = STATES.map(([k, l, g]) =>
    '<button class="chip ' + k + '" data-k="' + k + '" data-on="1" type="button" ' +
    'aria-pressed="true"><span class="g">' + g + '</span>' + l +
    ' <span class="n">' + (DATA.counts[k] || 0) + "</span></button>").join("");
  host.addEventListener("click", e => {
    const b = e.target.closest(".chip"); if (!b) return;
    const k = b.dataset.k;
    if (on.has(k) && on.size === STATES.length) { on.clear(); on.add(k); }
    else if (on.has(k)) { on.delete(k); if (!on.size) STATES.forEach(s => on.add(s[0])); }
    else on.add(k);
    host.querySelectorAll(".chip").forEach(c => {
      const v = on.has(c.dataset.k) ? "1" : "0";
      c.dataset.on = v; c.setAttribute("aria-pressed", v === "1" ? "true" : "false");
    });
    render();
  });
}

function match(r) {
  if (!on.has(r.s)) return false;
  if (!query) return true;
  const q = query.toLowerCase();
  return (r.k + " " + r.oq + " " + r.tq + " " + r.og + " " + r.tg + " " + r.w).toLowerCase().includes(q);
}

function rowHTML(r, showOwner) {
  const name = showOwner ? esc(r.k)
    : esc(r.k.includes("::") ? r.k.split("::").slice(1).join("::") : r.k);
  let h = '<div class="row">';
  h += '<div class="rname">' + name +
       (r.kind ? '<span class="rkind">' + esc(r.kind) + "</span>" : "") +
       (r.no > 1 ? '<span class="rkind">' + r.no + " overloads</span>" : "") + "</div>";
  h += '<div class="rtags">' + tag(r.s) + "</div>";
  if (r.tg || r.og) {
    h += '<div class="sigs">';
    h += '<div class="sig' + (r.tg ? "" : " empty") + '"><span class="lbl">ROS 2</span>' +
         (r.tg ? esc(r.tg) : "— no counterpart") + "</div>";
    h += '<div class="sig' + (r.og ? "" : " empty") + '"><span class="lbl">nano-ros</span>' +
         (r.og ? esc(r.og) : "— not implemented") + "</div>";
    h += "</div>";
  }
  if (r.p && r.p.length) {
    h += '<div class="prov-list"><span class="arrow">provided by →</span>' +
         r.p.map(x => "<code>" + esc(x) + "</code>").join("") + "</div>";
  }
  if (r.w) h += '<div class="why">' + prose(r.w) + "</div>";
  return h + "</div>";
}

function groupBlock(gname, rows, showOwner) {
  const states = [...new Set(rows.map(r => r.s))];
  const uniform = states.length === 1;
  const withWhy = rows.filter(r => r.w && !r.i);
  let h = '<details class="grp"' + (rows.length <= 6 ? " open" : "") + ">";
  h += "<summary>";
  h += '<span class="gname">' + esc(gname) + "</span>";
  h += '<span class="gmeta">' + rows.length + (rows.length === 1 ? " item" : " items") + "</span>";
  h += '<span class="gchips">' +
       STATES.filter(s => states.includes(s[0]))
             .map(s => tag(s[0]) + (uniform ? "" :
                  '<span class="gmeta">' + rows.filter(r => r.s === s[0]).length + "</span>")).join("") +
       "</span>";
  h += "</summary>";
  // group-level reason: stated once when the whole type shares one verdict
  if (uniform && withWhy.length) {
    h += '<p class="uniform"><em>All ' + rows.length + " " +
         LABEL[states[0]] + ".</em> " + prose(withWhy[0].w) + "</p>";
  }
  h += '<div class="rows">' + rows.map(r => rowHTML(r, showOwner)).join("") + "</div>";
  return h + "</details>";
}

function render() {
  const rows = DATA.rows.filter(match);
  document.getElementById("shown").textContent = rows.length;
  const host = document.getElementById("body");
  if (!rows.length) {
    host.innerHTML = '<p class="empty-note">No items match this filter.</p>'; return;
  }
  let h = "";
  if (DATA.layout === "flat") {
    const by = new Map();
    rows.forEach(r => { (by.get(r.g) || by.set(r.g, []).get(r.g)).push(r); });
    [...by.keys()].sort().forEach(g => { h += groupBlock(g, by.get(g), true); });
  } else {
    const secs = new Map();
    rows.forEach(r => {
      if (!secs.has(r.sec)) secs.set(r.sec, new Map());
      const m = secs.get(r.sec);
      (m.get(r.g) || m.set(r.g, []).get(r.g)).push(r);
    });
    const order = [...secs.keys()].sort((a, b) => {
      const ca = [...secs.get(a).values()].reduce((n, v) => n + v.length, 0);
      const cb = [...secs.get(b).values()].reduce((n, v) => n + v.length, 0);
      return cb - ca;
    });
    order.forEach(sec => {
      const m = secs.get(sec);
      const n = [...m.values()].reduce((a, v) => a + v.length, 0);
      h += '<section class="aud"><h2>' + esc(sec) +
           '<span class="c">' + n + " items · " + m.size + " groups</span></h2>";
      [...m.keys()].sort().forEach(g => { h += groupBlock(g, m.get(g), false); });
      h += "</section>";
    });
  }
  host.innerHTML = h;
}

buildChips();
document.getElementById("q").addEventListener("input", e => { query = e.target.value; render(); });
document.getElementById("expand").addEventListener("click", () => {
  const any = !document.querySelector(".grp:not([open])");
  document.querySelectorAll(".grp").forEach(d => { d.open = !any; });
});
render();
