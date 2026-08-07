@preconcurrency import WebKit

@MainActor
enum BrowserUserScripts {
    static let snapshot = WKUserScript(
        source: #"""
        (() => {
          if (window.__tillerSnapshot) return;
          window.__tillerSnapshot = function(generation) {
            const selector = 'a,button,input,textarea,select,[role="button"],[tabindex]';
            const nodes = Array.from(document.querySelectorAll(selector));
            return nodes.map((element, index) => {
              const ref = 'e' + (index + 1);
              element.setAttribute('data-tiller-ref', ref);
              element.setAttribute('data-tiller-generation', String(generation));
              const rect = element.getBoundingClientRect();
              const tag = element.tagName.toLowerCase();
              const role = element.getAttribute('role') ||
                (tag === 'a' ? 'link' : tag === 'button' ? 'button' : tag);
              const value = ('value' in element && element.value !== '') ? String(element.value) : null;
              const name = element.getAttribute('aria-label') ||
                element.getAttribute('name') || element.getAttribute('placeholder') ||
                (element.innerText || element.textContent || '').trim();
              return {ref, role, name, value,
                box: {x: rect.x, y: rect.y, width: rect.width, height: rect.height}};
            });
          };
        })();
        """#,
        injectionTime: .atDocumentEnd,
        forMainFrameOnly: true)

    static let console = WKUserScript(
        source: #"""
        (() => {
          const max = 200;
          const key = '__tillerConsoleBuffer';
          if (!window[key]) window[key] = [];
          for (const level of ['log', 'info', 'warn', 'error', 'debug']) {
            const original = console[level].bind(console);
            console[level] = (...args) => {
              window[key].push({level, text: args.map(String).join(' '), at: Date.now() / 1000});
              if (window[key].length > max) window[key].splice(0, window[key].length - max);
              original(...args);
            };
          }
        })();
        """#,
        injectionTime: .atDocumentStart,
        forMainFrameOnly: true)
}
