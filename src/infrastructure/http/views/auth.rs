use crate::infrastructure::http::theme;

pub const WEBAUTHN_JSON_SCRIPT: &str = r#"!function(){"use strict";var e={};function t(e){const t="==".slice(0,(4-e.length%4)%4),n=e.replace(/-/g,"+").replace(/_/g,"/")+t,r=atob(n),c=new ArrayBuffer(r.length),o=new Uint8Array(c);for(let e=0;e<r.length;e++)o[e]=r.charCodeAt(e);return c}function n(e){const t=new Uint8Array(e);let n="";for(const e of t)n+=String.fromCharCode(e);return btoa(n).replace(/\+/g,"-").replace(/\//g,"_").replace(/=/g,"")}function r(e,t,n){if("copy"===t)return n;if("convert"===t)return e(n);if(t instanceof Array)return n.map((n=>r(e,t[0],n)));if(t instanceof Object){const c={};for(const[o,i]of Object.entries(t))if(o in n)null!=n[o]?c[o]=r(e,i.schema,n[o]):c[o]=null;else if(i.required)throw new Error("Missing key: "+o);return c}}function c(e){return{required:!0,schema:e}}function o(e){return{required:!1,schema:e}}const i={type:c("copy"),id:c("convert"),transports:o("copy")},a={appid:o("copy"),appidExclude:o("copy"),credProps:o("copy")},s={appid:o("copy"),appidExclude:o("copy"),credProps:o("copy")},l={publicKey:c({rp:c("copy"),user:c({id:c("convert"),name:c("copy"),displayName:c("copy")}),challenge:c("convert"),pubKeyCredParams:c("copy"),timeout:o("copy"),excludeCredentials:o([i]),authenticatorSelection:o("copy"),attestation:o("copy"),extensions:o(a)}),signal:o("copy")},p={type:c("copy"),id:c("copy"),rawId:c("convert"),response:c({clientDataJSON:c("convert"),attestationObject:c("convert")}),clientExtensionResults:c(s)},u={mediation:o("copy"),publicKey:c({challenge:c("convert"),timeout:o("copy"),rpId:o("copy"),allowCredentials:o([i]),userVerification:o("copy"),extensions:o(a)}),signal:o("copy")},y={type:c("copy"),id:c("copy"),rawId:c("convert"),response:c({clientDataJSON:c("convert"),authenticatorData:c("convert"),signature:c("convert"),userHandle:c("convert")}),clientExtensionResults:c(s)},d={credentialCreationOptions:l,publicKeyCredentialWithAttestation:p,credentialRequestOptions:u,publicKeyCredentialWithAssertion:y};e.get=async function(e){return function(e){const t=e;return t.clientExtensionResults=e.getClientExtensionResults(),r(n,y,t)}(await navigator.credentials.get(function(e){return r(t,u,e)}(e)))},e.create=async function(e){return function(e){const t=e;return t.clientExtensionResults=e.getClientExtensionResults(),r(n,p,t)}(await navigator.credentials.create(function(e){return r(t,l,e)}(e)))},e.supported=function(){return!!(navigator.credentials&&navigator.credentials.create&&navigator.credentials.get&&window.PublicKeyCredential)},e.schema=d,globalThis.webauthnJSON=e;}();"#;

pub fn render_login_page(i18n: &crate::i18n::Translations, has_admin: bool) -> String {
    let (next_lang, lang_btn_label, lang_title) = if i18n.lang == "en" {
        ("pt-BR", "PT", "Mudar para Português")
    } else {
        ("en", "EN", "Switch to English")
    };

    let warning_box = if !has_admin {
        format!(
            r##"<div class="mb-6 p-4 rounded-lg bg-amber-500/10 border border-amber-500/30 text-amber-300 text-xs font-mono flex items-start gap-3">
                <span class="px-1.5 py-0.5 rounded bg-amber-500/20 text-amber-400 font-bold shrink-0">WARN</span>
                <span class="leading-relaxed">{}</span>
            </div>"##,
            i18n.login.no_admin_warning
        )
    } else {
        String::new()
    };

    let head_tags = theme::render_head(
        &i18n.login.page_title,
        r#"<script src="/assets/webauthn-json.js"></script>"#,
    );

    format!(
        r##"<!DOCTYPE html>
<html lang="{lang}" class="dark">
<head>
    {head_tags}
</head>
<body class="text-slate-100 min-h-screen flex items-center justify-center p-3 sm:p-6 antialiased font-sans selection:bg-rose-600 selection:text-white">
    <div class="w-full max-w-md sm:max-w-xl mx-auto my-auto py-2 sm:py-4">
        <!-- Interactive Auth Card -->
        <div class="w-full bg-surface border border-rose-500/20 rounded-2xl p-4 sm:p-6 shadow-2xl relative overflow-hidden">
            <!-- Top accent rule -->
            <div class="absolute top-0 left-0 right-0 h-[2px] bg-gradient-to-r from-transparent via-rose-500/60 to-transparent"></div>

            <!-- Top Header: Brand & Language -->
            <div class="flex items-center justify-between gap-3 mb-4 pb-3 border-b border-white/[0.06]">
                <div class="flex items-center gap-2.5">
                    <div class="w-7 h-7 sm:w-8 sm:h-8 rounded-lg bg-rose-600/10 border border-rose-500/30 flex items-center justify-center font-display font-bold text-base sm:text-lg text-rose-400 shrink-0">
                        狛
                    </div>
                    <div>
                        <span class="font-display font-bold text-sm sm:text-base text-white tracking-tight leading-none block">Komainu</span>
                        <span class="text-[10px] text-slate-400 font-mono">Gateway</span>
                    </div>
                </div>
                <a href="/set-lang?lang={next_lang}&redirect=/login" class="px-2 sm:px-2.5 py-1 rounded-md text-xs font-mono font-medium text-slate-300 hover:text-white bg-white/[0.04] hover:bg-white/[0.08] transition-colors duration-150 flex items-center gap-1.5 border border-white/[0.08] hover:border-rose-500/40" title="{lang_title}">
                    <svg class="w-3.5 h-3.5 text-slate-400" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M12 21a9.004 9.004 0 008.716-6.747M12 21a9.004 9.004 0 01-8.716-6.747M12 21c2.485 0 4.5-4.03 4.5-9S14.485 3 12 3m0 18c-2.485 0-4.5-4.03-4.5-9S9.515 3 12 3m0 0a8.997 8.997 0 017.843 4.582M12 3a8.997 8.997 0 00-7.843 4.582m15.686 0A11.953 11.953 0 0112 10.5c-2.998 0-5.74-1.1-7.843-2.918m15.686 0A8.959 8.959 0 0121 12c0 .778-.099 1.533-.284 2.253m0 0A17.919 17.919 0 0112 16.5c-3.162 0-6.133-.815-8.716-2.247m0 0A9.015 9.015 0 013 12c0-.778.099-1.533.284-2.253"/></svg>
                    <span>{lang_btn_label}</span>
                </a>
            </div>

            <div class="mb-5">
                <h1 class="text-xl sm:text-2xl font-display font-bold tracking-tight text-white">{header_title}</h1>
                <p class="text-xs text-slate-400 mt-0.5">{header_subtitle}</p>
            </div>

            {warning_box}

            <!-- Tabs de Método de Autenticação -->
            <div class="flex items-center gap-1.5 p-1 bg-surface-elevated rounded-lg border border-white/[0.06] mb-4">
                <button 
                    id="tab-btn-local" 
                    type="button" 
                    onclick="switchAuthTab('local')" 
                    class="flex-1 py-1.5 px-3 rounded-md text-xs font-mono font-medium transition-colors duration-150 flex items-center justify-center gap-1.5 bg-rose-500/15 text-rose-300 border border-rose-500/30 shadow-sm">
                    <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M9 12.75L11.25 15 15 9.75m-3-7.036A11.959 11.959 0 013.598 6 11.99 11.99 0 003 9.749c0 5.592 3.824 10.29 9 11.623 5.176-1.332 9-6.03 9-11.622 0-1.31-.21-2.571-.598-3.751h-.152c-3.196 0-6.1-1.248-8.25-3.285z"/></svg>
                    <span>{tab_local}</span>
                </button>
                <button 
                    id="tab-btn-qr" 
                    type="button" 
                    onclick="switchAuthTab('qr')" 
                    class="flex-1 py-1.5 px-3 rounded-md text-xs font-mono font-medium transition-colors duration-150 flex items-center justify-center gap-1.5 text-slate-400 hover:text-slate-200 hover:bg-white/[0.04] border border-transparent">
                    <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M3.75 4.875c0-.621.504-1.125 1.125-1.125h4.5c.621 0 1.125.504 1.125 1.125v4.5c0 .621-.504 1.125-1.125 1.125h-4.5A1.125 1.125 0 013.75 9.375v-4.5zM3.75 14.625c0-.621.504-1.125 1.125-1.125h4.5c.621 0 1.125.504 1.125 1.125v4.5c0 .621-.504 1.125-1.125 1.125h-4.5a1.125 1.125 0 01-1.125-1.125v-4.5zM13.5 4.875c0-.621.504-1.125 1.125-1.125h4.5c.621 0 1.125.504 1.125 1.125v4.5c0 .621-.504 1.125-1.125 1.125h-4.5A1.125 1.125 0 0113.5 9.375v-4.5z"/><path stroke-linecap="round" stroke-linejoin="round" d="M6.75 6.75h.75v.75h-.75v-.75zM6.75 16.5h.75v.75h-.75v-.75zM16.5 6.75h.75v.75h-.75v-.75zM13.5 13.5h.75v.75h-.75v-.75zM13.5 19.5h.75v.75h-.75v-.75zM19.5 13.5h.75v.75h-.75v-.75zM16.5 16.5h.75v.75h-.75v-.75zM19.5 19.5h.75v.75h-.75v-.75z"/></svg>
                    <span>{tab_qr}</span>
                </button>
            </div>

            <!-- Tab 1: Autenticação Local (WebAuthn neste dispositivo) -->
            <div id="tab-content-local">
                <p class="text-xs text-slate-400 font-sans leading-relaxed mb-4">
                    {description}
                </p>

                <div id="error-msg" class="hidden mb-3 p-2.5 rounded-lg bg-rose-500/10 border border-rose-500/30 text-rose-300 text-xs font-mono"></div>

                <button 
                    id="login-btn"
                    type="button" 
                    onclick="startLogin()"
                    class="w-full py-2.5 px-4 rounded-lg text-sm font-semibold transition-colors duration-150 shadow-sm flex items-center justify-center gap-2 bg-rose-600 hover:bg-rose-500 active:translate-y-px disabled:opacity-40 disabled:cursor-wait text-white border border-rose-400/30">
                    <svg class="w-4 h-4 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M15.75 5.25a3 3 0 013 3m3 0a6 6 0 01-7.029 5.912c-.563-.097-1.159.026-1.563.43L10.5 17.25H8.25v2.25H6v2.25H2.25v-2.818c0-.597.237-1.17.659-1.591l6.499-6.499c.404-.404.527-1 .43-1.563A6 6 0 1121.75 8.25z"/></svg>
                    <span>{btn_text}</span>
                </button>
            </div>

            <!-- Tab 2: Autenticação QR Code (Smartphone como chave) -->
            <div id="tab-content-qr" class="hidden text-center">
                <p class="text-xs text-slate-400 font-sans leading-relaxed mb-3">
                    {qr_desc}
                </p>

                <div class="relative inline-flex items-center justify-center p-2.5 bg-canvas rounded-xl border border-white/[0.08] shadow-inner mb-2.5">
                    <div id="qr-container" class="w-44 h-44 sm:w-52 sm:h-52 flex items-center justify-center">
                        <div class="w-6 h-6 border-2 border-rose-500 border-t-transparent rounded-full animate-spin"></div>
                    </div>
                    <div id="qr-overlay" class="hidden absolute inset-0 bg-canvas/90 backdrop-blur-sm rounded-xl flex flex-col items-center justify-center p-3 text-center">
                        <span id="qr-overlay-icon" class="text-2xl mb-1">⏳</span>
                        <p id="qr-overlay-text" class="text-xs font-mono text-slate-300"></p>
                        <button id="qr-reload-btn" type="button" onclick="loadQrSession()" class="hidden mt-2.5 px-3 py-1 bg-rose-600 hover:bg-rose-500 text-white rounded text-xs font-mono">
                            {qr_reload}
                        </button>
                    </div>
                </div>

                <div id="qr-status-indicator" class="flex items-center justify-center gap-1.5 text-xs font-mono text-slate-400">
                    <span id="qr-dot" class="w-2 h-2 rounded-full bg-amber-400 animate-pulse"></span>
                    <span id="qr-status-text">{qr_waiting}</span>
                </div>
            </div>
        </div>

        <p class="text-center text-[10px] sm:text-xs text-slate-400 font-mono mt-3">
            Komainu Gateway &copy; 2026
        </p>
    </div>

    <script>
    let activeTab = 'local';
    let qrPollInterval = null;
    let currentQrId = null;

    function switchAuthTab(tab) {{
        activeTab = tab;
        const btnLocal = document.getElementById('tab-btn-local');
        const btnQr = document.getElementById('tab-btn-qr');
        const contentLocal = document.getElementById('tab-content-local');
        const contentQr = document.getElementById('tab-content-qr');

        if (tab === 'local') {{
            btnLocal.className = "flex-1 py-1.5 px-3 rounded-md text-xs font-mono font-medium transition-colors duration-150 flex items-center justify-center gap-1.5 bg-rose-500/15 text-rose-300 border border-rose-500/30 shadow-sm";
            btnQr.className = "flex-1 py-1.5 px-3 rounded-md text-xs font-mono font-medium transition-colors duration-150 flex items-center justify-center gap-1.5 text-slate-400 hover:text-slate-200 hover:bg-white/[0.04] border border-transparent";
            contentLocal.classList.remove('hidden');
            contentQr.classList.add('hidden');
            stopQrPolling();
        }} else {{
            btnQr.className = "flex-1 py-1.5 px-3 rounded-md text-xs font-mono font-medium transition-colors duration-150 flex items-center justify-center gap-1.5 bg-rose-500/15 text-rose-300 border border-rose-500/30 shadow-sm";
            btnLocal.className = "flex-1 py-1.5 px-3 rounded-md text-xs font-mono font-medium transition-colors duration-150 flex items-center justify-center gap-1.5 text-slate-400 hover:text-slate-200 hover:bg-white/[0.04] border border-transparent";
            contentQr.classList.remove('hidden');
            contentLocal.classList.add('hidden');
            loadQrSession();
        }}
    }}

    function stopQrPolling() {{
        if (qrPollInterval) {{
            clearInterval(qrPollInterval);
            qrPollInterval = null;
        }}
    }}

    async function loadQrSession() {{
        stopQrPolling();
        const qrContainer = document.getElementById('qr-container');
        const overlay = document.getElementById('qr-overlay');
        const reloadBtn = document.getElementById('qr-reload-btn');
        const statusText = document.getElementById('qr-status-text');
        const dot = document.getElementById('qr-dot');

        overlay.classList.add('hidden');
        reloadBtn.classList.add('hidden');
        qrContainer.innerHTML = '<div class="w-6 h-6 border-2 border-rose-500 border-t-transparent rounded-full animate-spin"></div>';
        dot.className = "w-2 h-2 rounded-full bg-amber-400 animate-pulse";
        statusText.innerText = "{qr_waiting}";

        try {{
            const currentOrigin = window.location.origin;
            const res = await fetch(`/api/auth/qr_start?origin=${{encodeURIComponent(currentOrigin)}}`);
            if (!res.ok) throw new Error("Falha ao gerar QR Code");
            const data = await res.json();
            currentQrId = data.qr_id;
            qrContainer.innerHTML = data.svg;

            qrPollInterval = setInterval(pollQrStatus, 2000);
        }} catch (err) {{
            console.error(err);
            qrContainer.innerHTML = '<span class="text-rose-400 text-xs font-mono">Erro ao carregar QR</span>';
        }}
    }}

    async function pollQrStatus() {{
        if (!currentQrId) return;
        try {{
            const res = await fetch(`/api/auth/qr_poll?id=${{encodeURIComponent(currentQrId)}}`);
            if (!res.ok) return;
            const data = await res.json();
            const overlay = document.getElementById('qr-overlay');
            const overlayIcon = document.getElementById('qr-overlay-icon');
            const overlayText = document.getElementById('qr-overlay-text');
            const reloadBtn = document.getElementById('qr-reload-btn');
            const statusText = document.getElementById('qr-status-text');
            const dot = document.getElementById('qr-dot');

            if (data.status === 'scanned') {{
                dot.className = "w-2 h-2 rounded-full bg-cyan-400 animate-pulse";
                statusText.innerText = "{qr_scanned}";
            }} else if (data.status === 'approved') {{
                stopQrPolling();
                dot.className = "w-2 h-2 rounded-full bg-emerald-400";
                statusText.innerText = "{qr_approved}";
                overlayIcon.innerText = "✓";
                overlayText.innerText = "{qr_approved}";
                overlay.classList.remove('hidden');
                setTimeout(() => {{
                    window.location.href = '/';
                }}, 600);
            }} else if (data.status === 'expired') {{
                stopQrPolling();
                dot.className = "w-2 h-2 rounded-full bg-rose-500";
                statusText.innerText = "{qr_expired}";
                overlayIcon.innerText = "⚠️";
                overlayText.innerText = "{qr_expired}";
                reloadBtn.classList.remove('hidden');
                overlay.classList.remove('hidden');
            }}
        }} catch (err) {{
            console.error(err);
        }}
    }}

    async function startLogin() {{
        const btn = document.getElementById('login-btn');
        const errDiv = document.getElementById('error-msg');
        errDiv.classList.add('hidden');
        btn.disabled = true;
        btn.innerText = '{waiting_passkey}';

        try {{
            const startRes = await fetch('/api/auth/login_start');
            if (!startRes.ok) {{
                const err = await startRes.text();
                throw new Error(err || '{start_error}');
            }}
            const data = await startRes.json();
            const opts = data.challenge.publicKey ? data.challenge : {{ publicKey: data.challenge }};
            if (opts.publicKey) {{
                opts.publicKey.userVerification = "preferred";
                delete opts.publicKey.extensions;
                if (opts.publicKey.allowCredentials) {{
                    for (const cred of opts.publicKey.allowCredentials) {{
                        delete cred.transports;
                    }}
                }}
            }}
            const credential = await webauthnJSON.get(opts);

            const finishRes = await fetch('/api/auth/login_finish', {{
                method: 'POST',
                headers: {{ 'Content-Type': 'application/json' }},
                body: JSON.stringify({{ auth_id: data.auth_id, credential }})
            }});

            if (!finishRes.ok) {{
                const err = await finishRes.text();
                throw new Error(err || '{auth_error}');
            }}

            window.location.href = '/';
        }} catch (err) {{
            console.error(err);
            errDiv.innerText = err.message || '{default_error}';
            errDiv.classList.remove('hidden');
            btn.disabled = false;
            btn.innerText = '{btn_text}';
        }}
    }}
    </script>
</body>
</html>"##,
        lang = i18n.lang,
        head_tags = head_tags,
        header_title = i18n.login.header_title,
        header_subtitle = i18n.login.header_subtitle,
        warning_box = warning_box,
        description = i18n.login.description,
        btn_text = i18n.login.button,
        waiting_passkey = i18n.login.waiting_passkey,
        start_error = i18n.login.start_error,
        auth_error = i18n.login.auth_error,
        default_error = i18n.login.default_error,
        tab_local = i18n.login.tab_local,
        tab_qr = i18n.login.tab_qr,
        qr_desc = i18n.login.qr_desc,
        qr_waiting = i18n.login.qr_waiting,
        qr_scanned = i18n.login.qr_scanned,
        qr_approved = i18n.login.qr_approved,
        qr_expired = i18n.login.qr_expired,
        qr_reload = i18n.login.qr_reload,
        next_lang = next_lang,
        lang_btn_label = lang_btn_label,
        lang_title = lang_title,
    )
}

pub fn render_qr_expired_page(i18n: &crate::i18n::Translations) -> String {
    let head_tags = theme::render_head(&i18n.login.mobile_title, "");
    format!(
        r##"<!DOCTYPE html>
<html lang="{lang}" class="dark">
<head>
    {head_tags}
</head>
<body class="text-slate-100 min-h-screen flex items-center justify-center p-4 antialiased font-sans">
    <div class="max-w-md w-full bg-surface border border-rose-500/20 rounded-xl p-6 sm:p-8 shadow-2xl relative overflow-hidden text-center">
        <div class="w-12 h-12 rounded-full bg-amber-500/10 border border-amber-500/30 flex items-center justify-center text-amber-400 text-2xl mx-auto mb-4">
            ⚠️
        </div>
        <h1 class="text-xl font-display font-bold text-white mb-2">{qr_expired}</h1>
        <p class="text-xs text-slate-400 font-sans leading-relaxed mb-6">
            Esta solicitação de acesso não é mais válida. Por favor, volte ao computador e recarregue o QR Code na tela de login.
        </p>
    </div>
</body>
</html>"##,
        lang = i18n.lang,
        head_tags = head_tags,
        qr_expired = i18n.login.qr_expired,
    )
}

pub fn render_qr_login_page(i18n: &crate::i18n::Translations, token_str: &str) -> String {
    let head_tags = theme::render_head(
        &i18n.login.mobile_title,
        r#"<script src="/assets/webauthn-json.js"></script>"#,
    );

    format!(
        r##"<!DOCTYPE html>
<html lang="{lang}" class="dark">
<head>
    {head_tags}
</head>
<body class="text-slate-100 min-h-screen flex items-center justify-center p-4 antialiased font-sans selection:bg-rose-600 selection:text-white">
    <div class="max-w-md w-full bg-surface border border-rose-500/20 rounded-xl p-6 sm:p-8 shadow-2xl relative overflow-hidden">
        <!-- Top accent rule -->
        <div class="absolute top-0 left-0 right-0 h-[2px] bg-gradient-to-r from-transparent via-rose-500/60 to-transparent"></div>

        <div id="auth-box">
            <div class="flex items-center gap-3 mb-5">
                <div class="w-10 h-10 rounded-lg bg-rose-600/10 border border-rose-500/30 flex items-center justify-center font-display font-bold text-xl text-rose-400 shrink-0">
                    狛
                </div>
                <div>
                    <h1 class="text-lg sm:text-xl font-display font-bold tracking-tight text-white">{mobile_title}</h1>
                    <p class="text-xs text-slate-400 mt-0.5">{mobile_subtitle}</p>
                </div>
            </div>

            <div class="p-3.5 rounded-lg bg-canvas border border-white/[0.06] mb-6 text-xs font-mono text-slate-300 flex items-center gap-2.5">
                <svg class="w-4 h-4 text-rose-400 shrink-0" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z"/></svg>
                <span>Sessão de login pendente no computador</span>
            </div>

            <div id="error-msg" class="hidden mb-4 p-3 rounded-lg bg-rose-500/10 border border-rose-500/30 text-rose-300 text-xs font-mono"></div>

            <button 
                id="approve-btn"
                onclick="startMobileApproval('{token}')"
                class="w-full py-3.5 px-4 rounded-lg text-sm font-semibold transition-colors duration-150 shadow-sm flex items-center justify-center gap-2.5 bg-rose-600 hover:bg-rose-500 active:translate-y-px disabled:opacity-40 disabled:cursor-wait text-white border border-rose-400/30">
                <svg class="w-4 h-4 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z"/></svg>
                <span>{mobile_btn}</span>
            </button>
        </div>

        <div id="success-box" class="hidden text-center py-4">
            <div class="w-16 h-16 rounded-full bg-emerald-500/10 border border-emerald-500/30 flex items-center justify-center text-emerald-400 text-3xl mx-auto mb-4">
                ✓
            </div>
            <h2 class="text-lg font-display font-bold text-white mb-2">{mobile_success_title}</h2>
            <p class="text-xs text-slate-300 font-sans leading-relaxed mb-6">{mobile_success_desc}</p>
            <div class="py-2 px-3 rounded-lg bg-canvas border border-emerald-500/30 text-[11px] font-mono text-emerald-400 inline-block">
                SESSÃO DESBLOQUEADA NO COMPUTADOR
            </div>
        </div>
    </div>

    <script>
    async function startMobileApproval(token) {{
        const btn = document.getElementById('approve-btn');
        const errDiv = document.getElementById('error-msg');
        const authBox = document.getElementById('auth-box');
        const successBox = document.getElementById('success-box');
        errDiv.classList.add('hidden');
        btn.disabled = true;
        btn.innerText = '{waiting_passkey}';

        try {{
            const startRes = await fetch(`/api/auth/qr_mobile_start?token=${{encodeURIComponent(token)}}`);
            if (!startRes.ok) {{
                const err = await startRes.text();
                throw new Error(err || '{start_error}');
            }}
            const data = await startRes.json();
            const opts = data.challenge.publicKey ? data.challenge : {{ publicKey: data.challenge }};
            if (opts.publicKey) {{
                opts.publicKey.userVerification = "preferred";
                delete opts.publicKey.extensions;
                if (opts.publicKey.allowCredentials) {{
                    for (const cred of opts.publicKey.allowCredentials) {{
                        delete cred.transports;
                    }}
                }}
            }}
            const credential = await webauthnJSON.get(opts);

            const finishRes = await fetch('/api/auth/qr_mobile_finish', {{
                method: 'POST',
                headers: {{ 'Content-Type': 'application/json' }},
                body: JSON.stringify({{ token, auth_id: data.auth_id, credential }})
            }});

            if (!finishRes.ok) {{
                const err = await finishRes.text();
                throw new Error(err || '{auth_error}');
            }}

            authBox.classList.add('hidden');
            successBox.classList.remove('hidden');
        }} catch (err) {{
            console.error(err);
            errDiv.innerText = err.message || '{default_error}';
            errDiv.classList.remove('hidden');
            btn.disabled = false;
            btn.innerText = '{mobile_btn}';
        }}
    }}
    </script>
</body>
</html>"##,
        lang = i18n.lang,
        head_tags = head_tags,
        mobile_title = i18n.login.mobile_title,
        mobile_subtitle = i18n.login.mobile_subtitle,
        mobile_btn = i18n.login.mobile_btn,
        mobile_success_title = i18n.login.mobile_success_title,
        mobile_success_desc = i18n.login.mobile_success_desc,
        waiting_passkey = i18n.login.waiting_passkey,
        start_error = i18n.login.start_error,
        auth_error = i18n.login.auth_error,
        default_error = i18n.login.default_error,
        token = token_str,
    )
}

pub fn render_register_page(i18n: &crate::i18n::Translations, token: &str) -> String {
    let head_tags = theme::render_head(
        &i18n.register.page_title,
        r#"<script src="/assets/webauthn-json.js"></script>"#,
    );

    format!(
        r##"<!DOCTYPE html>
<html lang="{lang}" class="dark">
<head>
    {head_tags}
</head>
<body class="text-slate-100 min-h-screen flex items-center justify-center p-4 antialiased font-sans selection:bg-rose-600 selection:text-white">
    <div class="max-w-md w-full bg-surface border border-rose-500/20 rounded-xl p-6 sm:p-8 shadow-2xl relative overflow-hidden">
        <!-- Top accent rule -->
        <div class="absolute top-0 left-0 right-0 h-[2px] bg-gradient-to-r from-transparent via-rose-500/60 to-transparent"></div>

        <div class="flex items-center gap-2.5 mb-5 pb-4 border-b border-white/[0.06]">
            <div class="w-8 h-8 rounded-lg bg-rose-600/10 border border-rose-500/30 flex items-center justify-center font-display font-bold text-lg text-rose-400 shrink-0">
                狛
            </div>
            <div>
                <span class="font-display font-bold text-base text-white tracking-tight leading-none block">Komainu</span>
                <span class="text-[10px] text-rose-400 font-mono">Setup de Acesso</span>
            </div>
        </div>

        <div class="mb-5">
            <h1 class="text-xl font-display font-bold tracking-tight text-white">{header_title}</h1>
            <p class="text-xs text-slate-400 mt-0.5">{header_subtitle}</p>
        </div>

        <p class="text-sm text-slate-300/90 leading-relaxed mb-6 font-sans">
            {description}
        </p>

        <div id="error-msg" class="hidden mb-4 p-3 rounded-lg bg-rose-500/10 border border-rose-500/30 text-rose-300 text-xs font-mono"></div>

        <button 
            id="reg-btn"
            onclick="startRegistration('{token}')"
            class="w-full py-3.5 px-4 rounded-lg text-sm font-semibold transition-colors duration-150 shadow-sm flex items-center justify-center gap-2.5 bg-rose-600 hover:bg-rose-500 active:translate-y-px disabled:opacity-40 disabled:cursor-wait text-white border border-rose-400/30">
            <svg class="w-4 h-4 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"/></svg>
            <span>{btn_text}</span>
        </button>
    </div>

    <script>
    async function startRegistration(token) {{
        const btn = document.getElementById('reg-btn');
        const errDiv = document.getElementById('error-msg');
        errDiv.classList.add('hidden');
        btn.disabled = true;
        btn.innerText = '{creating_credential}';

        try {{
            const startRes = await fetch(`/api/auth/register_start?token=${{encodeURIComponent(token)}}`);
            if (!startRes.ok) {{
                const err = await startRes.text();
                throw new Error(err || '{start_error}');
            }}
            const data = await startRes.json();
            const opts = data.challenge.publicKey ? data.challenge : {{ publicKey: data.challenge }};
            if (opts.publicKey) {{
                delete opts.publicKey.extensions;
                opts.publicKey.authenticatorSelection = {{
                    residentKey: "preferred",
                    userVerification: "required"
                }};
            }}
            const credential = await webauthnJSON.create(opts);

            const finishRes = await fetch(`/api/auth/register_finish?token=${{encodeURIComponent(token)}}`, {{
                method: 'POST',
                headers: {{ 'Content-Type': 'application/json' }},
                body: JSON.stringify({{ reg_id: data.reg_id, credential }})
            }});

            if (!finishRes.ok) {{
                const err = await finishRes.text();
                throw new Error(err || '{save_error}');
            }}

            alert('{success_alert}');
            window.location.href = '/login';
        }} catch (err) {{
            console.error(err);
            errDiv.innerText = err.message || '{default_error}';
            errDiv.classList.remove('hidden');
            btn.disabled = false;
            btn.innerText = '{btn_text}';
        }}
    }}
    </script>
</body>
</html>"##,
        lang = i18n.lang,
        head_tags = head_tags,
        header_title = i18n.register.header_title,
        header_subtitle = i18n.register.header_subtitle,
        description = i18n.register.description,
        btn_text = i18n.register.button,
        token = token,
        creating_credential = i18n.register.creating_credential,
        start_error = i18n.register.start_error,
        save_error = i18n.register.save_error,
        success_alert = i18n.register.success_alert,
        default_error = i18n.register.default_error,
    )
}

