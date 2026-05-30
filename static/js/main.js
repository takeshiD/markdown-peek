function initializeMermaid() {
    window.mermaid.initialize({ startOnLoad: false, isLight: "default" });
    window.mermaid.run({ querySelector: "code.language-mermaid" });
}

function initializeMathJax() {
    window.MathJax.typeset();
}

function initializeHighlight() {
    if (!window.hljs) {
        return;
    }
    // Highlight every code block except mermaid diagrams and math blocks,
    // which are handled by mermaid/MathJax respectively.
    document.querySelectorAll("pre code").forEach(function (block) {
        if (
            block.classList.contains("language-mermaid") ||
            block.classList.contains("language-math")
        ) {
            return;
        }
        window.hljs.highlightElement(block);
    });
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
                socket.close();
                window.location.reload();
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
