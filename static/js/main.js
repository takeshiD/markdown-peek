function initializeMermaid() {
    window.mermaid.initialize({ startOnLoad: false, isLight: "default" });
    window.mermaid.run({ querySelector: "code.language-mermaid" });
}

function initializeMathJax() {
    window.MathJax.typeset();
}

function highlightWithin(root) {
    if (!window.hljs) {
        return;
    }
    root.querySelectorAll("pre code").forEach(function (block) {
        if (
            block.classList.contains("language-mermaid") ||
            block.classList.contains("language-math") ||
            block.dataset.highlighted === "yes"
        ) {
            return;
        }
        window.hljs.highlightElement(block);
    });
}

function initializeHighlight() {
    highlightWithin(document);
}

const MDPEEK_THEME_KEY = "mdpeek-theme";

// Lucide icons (https://lucide.dev) embedded inline so we don't depend on a CDN.
const LUCIDE_MOON =
    '<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="lucide lucide-moon"><path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"/></svg>';
const LUCIDE_SUN =
    '<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="lucide lucide-sun"><circle cx="12" cy="12" r="4"/><path d="M12 2v2"/><path d="M12 20v2"/><path d="m4.93 4.93 1.41 1.41"/><path d="m17.66 17.66 1.41 1.41"/><path d="M2 12h2"/><path d="M20 12h2"/><path d="m6.34 17.66-1.41 1.41"/><path d="m19.07 4.93-1.41 1.41"/></svg>';

function currentTheme() {
    const stored = localStorage.getItem(MDPEEK_THEME_KEY);
    if (stored === "light" || stored === "dark") {
        return stored;
    }
    if (window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches) {
        return "dark";
    }
    return "light";
}

function applyTheme(theme) {
    const isDark = theme === "dark";
    const lightCss = document.getElementById("mdpeek-theme-light");
    const darkCss = document.getElementById("mdpeek-theme-dark");
    const hljsLight = document.getElementById("mdpeek-hljs-light");
    const hljsDark = document.getElementById("mdpeek-hljs-dark");
    if (lightCss) lightCss.disabled = isDark;
    if (darkCss) darkCss.disabled = !isDark;
    if (hljsLight) hljsLight.disabled = isDark;
    if (hljsDark) hljsDark.disabled = !isDark;

    const toggle = document.getElementById("mdpeek-theme-toggle");
    if (toggle) {
        // Show the icon for the theme you'd switch *to*.
        toggle.innerHTML = isDark ? LUCIDE_SUN : LUCIDE_MOON;
    }
}

function initializeTheme() {
    let theme = currentTheme();
    applyTheme(theme);
    const toggle = document.getElementById("mdpeek-theme-toggle");
    if (toggle) {
        toggle.addEventListener("click", function () {
            theme = theme === "dark" ? "light" : "dark";
            localStorage.setItem(MDPEEK_THEME_KEY, theme);
            applyTheme(theme);
        });
    }
}

function initializeToc() {
    const article = document.querySelector(".markdown-body");
    if (!article) {
        return;
    }
    // Remove any prior TOC so this function can be re-run after a diff update.
    const existing = document.getElementById("mdpeek-toc");
    if (existing) {
        existing.remove();
    }
    const headings = article.querySelectorAll("h1, h2, h3, h4, h5, h6");
    // Only show the table of contents when there is more than one heading.
    if (headings.length < 2) {
        return;
    }

    const toc = document.createElement("nav");
    toc.id = "mdpeek-toc";

    const title = document.createElement("div");
    title.id = "mdpeek-toc-title";
    title.innerHTML = "<span>Contents</span><span aria-hidden=\"true\">▾</span>";
    toc.appendChild(title);

    const list = document.createElement("ul");
    headings.forEach(function (heading) {
        if (!heading.id) {
            return;
        }
        const level = heading.tagName.toLowerCase();
        const item = document.createElement("li");
        item.className = "toc-" + level;
        const link = document.createElement("a");
        link.href = "#" + heading.id;
        // Use only the heading's text, ignoring the anchor link octicon.
        link.textContent = (heading.textContent || "").trim();
        link.addEventListener("click", function (event) {
            event.preventDefault();
            const target = document.getElementById(heading.id);
            if (target) {
                target.scrollIntoView({ behavior: "smooth", block: "start" });
                history.replaceState(null, "", "#" + heading.id);
            }
        });
        item.appendChild(link);
        list.appendChild(item);
    });

    if (!list.children.length) {
        return;
    }
    toc.appendChild(list);

    title.addEventListener("click", function () {
        toc.classList.toggle("mdpeek-toc-collapsed");
        const caret = title.lastElementChild;
        if (caret) {
            caret.textContent = toc.classList.contains("mdpeek-toc-collapsed") ? "▸" : "▾";
        }
    });

    document.body.appendChild(toc);
}

const MDPEEK_DIFF_CLASSES = [
    "mdpeek-diff-added",
    "mdpeek-diff-modified",
    "mdpeek-diff-removed",
    "mdpeek-diff-fade-in",
];

// Strip transient diff classes so a node's outerHTML is comparable across
// successive updates regardless of whether it was last animated.
function diffKey(node) {
    const clone = node.cloneNode(true);
    MDPEEK_DIFF_CLASSES.forEach(function (cls) {
        clone.classList && clone.classList.remove(cls);
        clone.querySelectorAll && clone.querySelectorAll("." + cls).forEach(function (n) {
            n.classList.remove(cls);
        });
    });
    return clone.outerHTML;
}

// LCS-based diff producing keep/add/remove ops over the two child sequences.
function diffBlocks(oldNodes, newNodes) {
    const oldKeys = oldNodes.map(diffKey);
    const newKeys = newNodes.map(diffKey);
    const m = oldKeys.length;
    const n = newKeys.length;
    const dp = Array.from({ length: m + 1 }, function () {
        return new Array(n + 1).fill(0);
    });
    for (let i = m - 1; i >= 0; i--) {
        for (let j = n - 1; j >= 0; j--) {
            if (oldKeys[i] === newKeys[j]) {
                dp[i][j] = dp[i + 1][j + 1] + 1;
            } else {
                dp[i][j] = Math.max(dp[i + 1][j], dp[i][j + 1]);
            }
        }
    }
    const ops = [];
    let i = 0;
    let j = 0;
    while (i < m && j < n) {
        if (oldKeys[i] === newKeys[j]) {
            ops.push({ type: "keep", old: oldNodes[i] });
            i++;
            j++;
        } else if (dp[i + 1][j] >= dp[i][j + 1]) {
            ops.push({ type: "remove", old: oldNodes[i] });
            i++;
        } else {
            ops.push({ type: "add", new: newNodes[j] });
            j++;
        }
    }
    while (i < m) {
        ops.push({ type: "remove", old: oldNodes[i++] });
    }
    while (j < n) {
        ops.push({ type: "add", new: newNodes[j++] });
    }
    return ops;
}

// Pair an adjacent remove+add (in either order) of the same tag into a single
// "modify" op so the user sees a yellow flash instead of red+green.
function collapseModifications(ops) {
    const out = [];
    let i = 0;
    while (i < ops.length) {
        const cur = ops[i];
        const next = ops[i + 1];
        if (next) {
            if (
                cur.type === "remove" &&
                next.type === "add" &&
                cur.old.tagName === next.new.tagName
            ) {
                out.push({ type: "modify", old: cur.old, new: next.new });
                i += 2;
                continue;
            }
            if (
                cur.type === "add" &&
                next.type === "remove" &&
                cur.new.tagName === next.old.tagName
            ) {
                out.push({ type: "modify", old: next.old, new: cur.new });
                i += 2;
                continue;
            }
        }
        out.push(cur);
        i++;
    }
    return out;
}

function clearDiffClasses(node) {
    MDPEEK_DIFF_CLASSES.forEach(function (cls) {
        node.classList.remove(cls);
    });
}

function applyDiff(newHtml) {
    const article = document.querySelector("article.markdown-body");
    if (!article) {
        return;
    }
    const parser = new DOMParser();
    const newDoc = parser.parseFromString(newHtml, "text/html");
    const newArticle = newDoc.querySelector("article.markdown-body");
    if (!newArticle) {
        return;
    }

    const newTitle = newDoc.querySelector("title");
    if (newTitle && newTitle.textContent) {
        document.title = newTitle.textContent;
    }

    // Ignore stale removed-but-still-animating nodes from a previous update.
    const oldNodes = Array.from(article.children).filter(function (n) {
        return !n.classList.contains("mdpeek-diff-removed");
    });
    const newNodes = Array.from(newArticle.children);

    const ops = collapseModifications(diffBlocks(oldNodes, newNodes));

    const finalChildren = [];
    const toRemove = [];
    const toHighlight = [];

    ops.forEach(function (op) {
        if (op.type === "keep") {
            clearDiffClasses(op.old);
            finalChildren.push(op.old);
        } else if (op.type === "add") {
            const node = document.adoptNode(op.new);
            node.classList.add("mdpeek-diff-added", "mdpeek-diff-fade-in");
            finalChildren.push(node);
            toHighlight.push(node);
        } else if (op.type === "modify") {
            const node = document.adoptNode(op.new);
            node.classList.add("mdpeek-diff-modified", "mdpeek-diff-fade-in");
            finalChildren.push(node);
            toHighlight.push(node);
        } else if (op.type === "remove") {
            op.old.classList.add("mdpeek-diff-removed");
            finalChildren.push(op.old);
            toRemove.push(op.old);
        }
    });

    article.replaceChildren.apply(article, finalChildren);

    toHighlight.forEach(function (node) {
        highlightWithin(node);
        if (window.mermaid) {
            try {
                window.mermaid.run({
                    nodes: node.querySelectorAll("code.language-mermaid"),
                });
            } catch (e) {
                // Mermaid throws for nodes it has already processed; ignore.
            }
        }
        if (window.MathJax && window.MathJax.typesetPromise) {
            window.MathJax.typesetPromise([node]).catch(function () {});
        }
    });

    initializeToc();

    setTimeout(function () {
        toRemove.forEach(function (n) {
            if (n.parentNode) {
                n.parentNode.removeChild(n);
            }
        });
        article
            .querySelectorAll(".mdpeek-diff-added, .mdpeek-diff-modified, .mdpeek-diff-fade-in")
            .forEach(clearDiffClasses);
    }, 1200);
}

async function fetchAndDiff() {
    try {
        const response = await fetch(window.location.href, {
            cache: "no-store",
            headers: { Accept: "text/html" },
        });
        if (!response.ok) {
            throw new Error("HTTP " + response.status);
        }
        const html = await response.text();
        applyDiff(html);
    } catch (err) {
        console.log("Diff update failed, falling back to reload: " + err);
        window.location.reload();
    }
}

(function() {
    initializeTheme();
    initializeMermaid();
    initializeHighlight();
    initializeToc();
    // initializeMathJax();

    var RECONNECT_INTERVAL_MS = 3000;
    var reconnectTimer = null;

    function connectWebSocket() {
        var wsUrl = "ws://" + window.location.host + "/ws";
        console.log("Creating WebSocket: " + wsUrl);
        var socket = new WebSocket(wsUrl);

        socket.onopen = function(event) {
            console.log("WebSocket open: " + event.type);
            if (reconnectTimer !== null) {
                clearTimeout(reconnectTimer);
                reconnectTimer = null;
            }
        };

        socket.onmessage = function(event) {
            console.log("WebSocket message: " + event.data);
            if (event.data === "reload") {
                fetchAndDiff();
            }
        };

        socket.onerror = function(event) {
            console.log("WebSocket error: " + event.type);
        };

        socket.onclose = function(event) {
            console.log("WebSocket closed (code=" + event.code + "). Reconnecting in " + RECONNECT_INTERVAL_MS + "ms...");
            reconnectTimer = setTimeout(connectWebSocket, RECONNECT_INTERVAL_MS);
        };
    }

    connectWebSocket();
}());
