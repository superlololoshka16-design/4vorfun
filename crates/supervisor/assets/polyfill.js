(function (P) {
    "use strict";
    var el = function () {
        return {
            style: {},
            setAttribute: function () { },
            getAttribute: function () { return null; },
            appendChild: function (c) { return c; },
            removeChild: function (c) { return c; },
            addEventListener: function () { },
            removeEventListener: function () { },
            getContext: function (kind) {
                if (kind !== "webgl" && kind !== "experimental-webgl" && kind !== "2d") { return null; }
                if (kind === "2d") { return { fillRect: function () { }, fillText: function () { }, getImageData: function () { return { data: [] }; } }; }
                var gl = {
                    getParameter: function (p) {
                        if (p === 37445) { return P().webglVendor; }
                        if (p === 37446) { return P().webglRenderer; }
                        return 0;
                    },
                    getExtension: function (n) {
                        if (n === "WEBGL_debug_renderer_info") { return { UNMASKED_VENDOR_WEBGL: 37445, UNMASKED_RENDERER_WEBGL: 37446 }; }
                        return null;
                    },
                    getShaderPrecisionFormat: function () { return { rangeMin: 127, rangeMax: 127, precision: 23 }; },
                    createShader: function () { return {}; },
                    shaderSource: function () { },
                    compileShader: function () { },
                    getShaderParameter: function () { return true; },
                    createProgram: function () { return {}; },
                    attachShader: function () { },
                    linkProgram: function () { },
                    getProgramParameter: function () { return true; }
                };
                return gl;
            },
            toDataURL: function () { return "data:image/png;base64," + P().canvasHash; }
        };
    };
    var d = globalThis.document;
    d.createElement = function () { return el(); };
    d.createElementNS = function () { return el(); };
    d.addEventListener = function () { };
    d.removeEventListener = function () { };
    globalThis.window = globalThis;
    globalThis.self = globalThis;
    var B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    globalThis.history = { length: 1, state: null, pushState: function () { }, replaceState: function () { }, back: function () { }, forward: function () { } };
    globalThis.setTimeout = function (fn) { if (typeof fn === "function") { try { fn(); } catch (e) { } } return 0; };
    globalThis.setInterval = function (fn) { if (typeof fn === "function") { try { fn(); } catch (e) { } } return 0; };
    globalThis.clearTimeout = function () { };
    globalThis.clearInterval = function () { };
    globalThis.setImmediate = function (fn) { if (typeof fn === "function") { try { fn(); } catch (e) { } } return 0; };
    globalThis.queueMicrotask = function (fn) { if (typeof fn === "function") { try { fn(); } catch (e) { } } };
    globalThis.requestAnimationFrame = function (fn) { if (typeof fn === "function") { try { fn(performance.now()); } catch (e) { } } return 0; };
    globalThis.cancelAnimationFrame = function () { };
    globalThis.addEventListener = function (type, fn) { };
    globalThis.removeEventListener = function (type, fn) { };
    globalThis.atob = function (s) {
        var out = [], bits = 0, acc = 0;
        for (var i = 0; i < s.length; i++) {
            var v = B64.indexOf(s.charAt(i));
            if (v < 0) { continue; }
            acc = (acc << 6) | v;
            bits += 6;
            if (bits >= 8) {
                bits -= 8;
                out.push((acc >> bits) & 255);
            }
        }
        var str = "";
        for (var j = 0; j < out.length; j++) { str += String.fromCharCode(out[j]); }
        return str;
    };
    globalThis.btoa = function (s) {
        var out = "", i;
        for (i = 0; i + 2 < s.length; i += 3) {
            var n = (s.charCodeAt(i) << 16) | (s.charCodeAt(i + 1) << 8) | s.charCodeAt(i + 2);
            out += B64.charAt(n >> 18) + B64.charAt((n >> 12) & 63) + B64.charAt((n >> 6) & 63) + B64.charAt(n & 63);
        }
        var tail = s.length - i;
        if (tail === 1) {
            var k = s.charCodeAt(i) << 16;
            out += B64.charAt(k >> 18) + B64.charAt((k >> 12) & 63) + B64.charAt((k >> 6) & 63) + "==";
        } else if (tail === 2) {
            var m = (s.charCodeAt(i) << 16) | (s.charCodeAt(i + 1) << 8);
            out += B64.charAt(m >> 18) + B64.charAt((m >> 12) & 63) + B64.charAt((m >> 6) & 63) + B64.charAt(m & 63);
        }
        return out;
    };
});
