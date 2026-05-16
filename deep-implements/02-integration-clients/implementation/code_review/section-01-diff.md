diff --git a/02-integration-clients/bun.lock b/02-integration-clients/bun.lock
new file mode 100644
index 0000000..ed7bd76
--- /dev/null
+++ b/02-integration-clients/bun.lock
@@ -0,0 +1,222 @@
+{
+  "lockfileVersion": 1,
+  "configVersion": 1,
+  "workspaces": {
+    "": {
+      "name": "02-integration-clients",
+      "dependencies": {
+        "grammy": "^1.36.0",
+      },
+      "devDependencies": {
+        "@types/node": "^20.17.0",
+        "typescript": "^5.8.0",
+        "vitest": "^2.1.0",
+      },
+    },
+  },
+  "packages": {
+    "@esbuild/aix-ppc64": ["@esbuild/aix-ppc64@0.21.5", "", { "os": "aix", "cpu": "ppc64" }, "sha512-1SDgH6ZSPTlggy1yI6+Dbkiz8xzpHJEVAlF/AM1tHPLsf5STom9rwtjE4hKAF20FfXXNTFqEYXyJNWh1GiZedQ=="],
+
+    "@esbuild/android-arm": ["@esbuild/android-arm@0.21.5", "", { "os": "android", "cpu": "arm" }, "sha512-vCPvzSjpPHEi1siZdlvAlsPxXl7WbOVUBBAowWug4rJHb68Ox8KualB+1ocNvT5fjv6wpkX6o/iEpbDrf68zcg=="],
+
+    "@esbuild/android-arm64": ["@esbuild/android-arm64@0.21.5", "", { "os": "android", "cpu": "arm64" }, "sha512-c0uX9VAUBQ7dTDCjq+wdyGLowMdtR/GoC2U5IYk/7D1H1JYC0qseD7+11iMP2mRLN9RcCMRcjC4YMclCzGwS/A=="],
+
+    "@esbuild/android-x64": ["@esbuild/android-x64@0.21.5", "", { "os": "android", "cpu": "x64" }, "sha512-D7aPRUUNHRBwHxzxRvp856rjUHRFW1SdQATKXH2hqA0kAZb1hKmi02OpYRacl0TxIGz/ZmXWlbZgjwWYaCakTA=="],
+
+    "@esbuild/darwin-arm64": ["@esbuild/darwin-arm64@0.21.5", "", { "os": "darwin", "cpu": "arm64" }, "sha512-DwqXqZyuk5AiWWf3UfLiRDJ5EDd49zg6O9wclZ7kUMv2WRFr4HKjXp/5t8JZ11QbQfUS6/cRCKGwYhtNAY88kQ=="],
+
+    "@esbuild/darwin-x64": ["@esbuild/darwin-x64@0.21.5", "", { "os": "darwin", "cpu": "x64" }, "sha512-se/JjF8NlmKVG4kNIuyWMV/22ZaerB+qaSi5MdrXtd6R08kvs2qCN4C09miupktDitvh8jRFflwGFBQcxZRjbw=="],
+
+    "@esbuild/freebsd-arm64": ["@esbuild/freebsd-arm64@0.21.5", "", { "os": "freebsd", "cpu": "arm64" }, "sha512-5JcRxxRDUJLX8JXp/wcBCy3pENnCgBR9bN6JsY4OmhfUtIHe3ZW0mawA7+RDAcMLrMIZaf03NlQiX9DGyB8h4g=="],
+
+    "@esbuild/freebsd-x64": ["@esbuild/freebsd-x64@0.21.5", "", { "os": "freebsd", "cpu": "x64" }, "sha512-J95kNBj1zkbMXtHVH29bBriQygMXqoVQOQYA+ISs0/2l3T9/kj42ow2mpqerRBxDJnmkUDCaQT/dfNXWX/ZZCQ=="],
+
+    "@esbuild/linux-arm": ["@esbuild/linux-arm@0.21.5", "", { "os": "linux", "cpu": "arm" }, "sha512-bPb5AHZtbeNGjCKVZ9UGqGwo8EUu4cLq68E95A53KlxAPRmUyYv2D6F0uUI65XisGOL1hBP5mTronbgo+0bFcA=="],
+
+    "@esbuild/linux-arm64": ["@esbuild/linux-arm64@0.21.5", "", { "os": "linux", "cpu": "arm64" }, "sha512-ibKvmyYzKsBeX8d8I7MH/TMfWDXBF3db4qM6sy+7re0YXya+K1cem3on9XgdT2EQGMu4hQyZhan7TeQ8XkGp4Q=="],
+
+    "@esbuild/linux-ia32": ["@esbuild/linux-ia32@0.21.5", "", { "os": "linux", "cpu": "ia32" }, "sha512-YvjXDqLRqPDl2dvRODYmmhz4rPeVKYvppfGYKSNGdyZkA01046pLWyRKKI3ax8fbJoK5QbxblURkwK/MWY18Tg=="],
+
+    "@esbuild/linux-loong64": ["@esbuild/linux-loong64@0.21.5", "", { "os": "linux", "cpu": "none" }, "sha512-uHf1BmMG8qEvzdrzAqg2SIG/02+4/DHB6a9Kbya0XDvwDEKCoC8ZRWI5JJvNdUjtciBGFQ5PuBlpEOXQj+JQSg=="],
+
+    "@esbuild/linux-mips64el": ["@esbuild/linux-mips64el@0.21.5", "", { "os": "linux", "cpu": "none" }, "sha512-IajOmO+KJK23bj52dFSNCMsz1QP1DqM6cwLUv3W1QwyxkyIWecfafnI555fvSGqEKwjMXVLokcV5ygHW5b3Jbg=="],
+
+    "@esbuild/linux-ppc64": ["@esbuild/linux-ppc64@0.21.5", "", { "os": "linux", "cpu": "ppc64" }, "sha512-1hHV/Z4OEfMwpLO8rp7CvlhBDnjsC3CttJXIhBi+5Aj5r+MBvy4egg7wCbe//hSsT+RvDAG7s81tAvpL2XAE4w=="],
+
+    "@esbuild/linux-riscv64": ["@esbuild/linux-riscv64@0.21.5", "", { "os": "linux", "cpu": "none" }, "sha512-2HdXDMd9GMgTGrPWnJzP2ALSokE/0O5HhTUvWIbD3YdjME8JwvSCnNGBnTThKGEB91OZhzrJ4qIIxk/SBmyDDA=="],
+
+    "@esbuild/linux-s390x": ["@esbuild/linux-s390x@0.21.5", "", { "os": "linux", "cpu": "s390x" }, "sha512-zus5sxzqBJD3eXxwvjN1yQkRepANgxE9lgOW2qLnmr8ikMTphkjgXu1HR01K4FJg8h1kEEDAqDcZQtbrRnB41A=="],
+
+    "@esbuild/linux-x64": ["@esbuild/linux-x64@0.21.5", "", { "os": "linux", "cpu": "x64" }, "sha512-1rYdTpyv03iycF1+BhzrzQJCdOuAOtaqHTWJZCWvijKD2N5Xu0TtVC8/+1faWqcP9iBCWOmjmhoH94dH82BxPQ=="],
+
+    "@esbuild/netbsd-x64": ["@esbuild/netbsd-x64@0.21.5", "", { "os": "none", "cpu": "x64" }, "sha512-Woi2MXzXjMULccIwMnLciyZH4nCIMpWQAs049KEeMvOcNADVxo0UBIQPfSmxB3CWKedngg7sWZdLvLczpe0tLg=="],
+
+    "@esbuild/openbsd-x64": ["@esbuild/openbsd-x64@0.21.5", "", { "os": "openbsd", "cpu": "x64" }, "sha512-HLNNw99xsvx12lFBUwoT8EVCsSvRNDVxNpjZ7bPn947b8gJPzeHWyNVhFsaerc0n3TsbOINvRP2byTZ5LKezow=="],
+
+    "@esbuild/sunos-x64": ["@esbuild/sunos-x64@0.21.5", "", { "os": "sunos", "cpu": "x64" }, "sha512-6+gjmFpfy0BHU5Tpptkuh8+uw3mnrvgs+dSPQXQOv3ekbordwnzTVEb4qnIvQcYXq6gzkyTnoZ9dZG+D4garKg=="],
+
+    "@esbuild/win32-arm64": ["@esbuild/win32-arm64@0.21.5", "", { "os": "win32", "cpu": "arm64" }, "sha512-Z0gOTd75VvXqyq7nsl93zwahcTROgqvuAcYDUr+vOv8uHhNSKROyU961kgtCD1e95IqPKSQKH7tBTslnS3tA8A=="],
+
+    "@esbuild/win32-ia32": ["@esbuild/win32-ia32@0.21.5", "", { "os": "win32", "cpu": "ia32" }, "sha512-SWXFF1CL2RVNMaVs+BBClwtfZSvDgtL//G/smwAc5oVK/UPu2Gu9tIaRgFmYFFKrmg3SyAjSrElf0TiJ1v8fYA=="],
+
+    "@esbuild/win32-x64": ["@esbuild/win32-x64@0.21.5", "", { "os": "win32", "cpu": "x64" }, "sha512-tQd/1efJuzPC6rCFwEvLtci/xNFcTZknmXs98FYDfGE4wP9ClFV98nyKrzJKVPMhdDnjzLhdUyMX4PsQAPjwIw=="],
+
+    "@grammyjs/types": ["@grammyjs/types@3.26.0", "", {}, "sha512-jlnyfxfev/2o68HlvAGRocAXgdPPX5QabG7jZlbqC2r9DZyWBfzTlg+nu3O3Fy4EhgLWu28hZ/8wr7DsNamP9A=="],
+
+    "@jridgewell/sourcemap-codec": ["@jridgewell/sourcemap-codec@1.5.5", "", {}, "sha512-cYQ9310grqxueWbl+WuIUIaiUaDcj7WOq5fVhEljNVgRfOUhY9fy2zTvfoqWsnebh8Sl70VScFbICvJnLKB0Og=="],
+
+    "@rollup/rollup-android-arm-eabi": ["@rollup/rollup-android-arm-eabi@4.60.4", "", { "os": "android", "cpu": "arm" }, "sha512-F5QXMSiFebS9hKZj02XhWLLnRpJ3B3AROP0tWbFBSj+6kCbg5m9j5JoHKd4mmSVy5mS/IMQloYgYxCuJC0fxEQ=="],
+
+    "@rollup/rollup-android-arm64": ["@rollup/rollup-android-arm64@4.60.4", "", { "os": "android", "cpu": "arm64" }, "sha512-GxxTKApUpzRhof7poWvCJHRF51C67u1R7D6DiluBE8wKU1u5GWE8t+v81JvJYtbawoBFX1hLv5Ei4eVjkWokaw=="],
+
+    "@rollup/rollup-darwin-arm64": ["@rollup/rollup-darwin-arm64@4.60.4", "", { "os": "darwin", "cpu": "arm64" }, "sha512-tua0TaJxMOB1R0V0RS1jFZ/RpURFDJIOR2A6jWwQeawuFyS4gBW+rntLRaQd0EQ4bd6Vp44Z2rXW+YYDBsj6IA=="],
+
+    "@rollup/rollup-darwin-x64": ["@rollup/rollup-darwin-x64@4.60.4", "", { "os": "darwin", "cpu": "x64" }, "sha512-CSKq7MsP+5PFIcydhAiR1K0UhEI1A2jWXVKHPCBZ151yOutENwvnPocgVHkivu2kviURtCEB6zUQw0vs8RrhMg=="],
+
+    "@rollup/rollup-freebsd-arm64": ["@rollup/rollup-freebsd-arm64@4.60.4", "", { "os": "freebsd", "cpu": "arm64" }, "sha512-+O8OkVdyvXMtJEciu2wS/pzm1IxntEEQx3z5TAVy4l32G0etZn+RsA48ARRrFm6Ri8fvqPQfgrvNxSjKAbnd3g=="],
+
+    "@rollup/rollup-freebsd-x64": ["@rollup/rollup-freebsd-x64@4.60.4", "", { "os": "freebsd", "cpu": "x64" }, "sha512-Iw3oMskH3AfNuhU0MSN7vNbdi4me/NiYo2azqPz/Le16zHSa+3RRmliCMWWQmh4lcndccU40xcJuTYJZxNo/lw=="],
+
+    "@rollup/rollup-linux-arm-gnueabihf": ["@rollup/rollup-linux-arm-gnueabihf@4.60.4", "", { "os": "linux", "cpu": "arm" }, "sha512-EIPRXTVQpHyF8WOo219AD2yEltPehLTcTMz2fn6JsatLYSzQf00hj3rulF+yauOlF9/FtM2WpkT/hJh/KJFGhA=="],
+
+    "@rollup/rollup-linux-arm-musleabihf": ["@rollup/rollup-linux-arm-musleabihf@4.60.4", "", { "os": "linux", "cpu": "arm" }, "sha512-J3Yh9PzzF1Ovah2At+lHiGQdsYgArxBbXv/zHfSyaiFQEqvNv7DcW98pCrmdjCZBrqBiKrKKe2V+aaSGWuBe/w=="],
+
+    "@rollup/rollup-linux-arm64-gnu": ["@rollup/rollup-linux-arm64-gnu@4.60.4", "", { "os": "linux", "cpu": "arm64" }, "sha512-BFDEZMYfUvLn37ONE1yMBojPxnMlTFsdyNoqncT0qFq1mAfllL+ATMMJd8TeuVMiX84s1KbcxcZbXInmcO2mRg=="],
+
+    "@rollup/rollup-linux-arm64-musl": ["@rollup/rollup-linux-arm64-musl@4.60.4", "", { "os": "linux", "cpu": "arm64" }, "sha512-pc9EYOSlOgdQ2uPl1o9PF6/kLSgaUosia7gOuS8mB69IxJvlclko1MECXysjs5ryez1/5zjYqx3+xYU0TU6R1A=="],
+
+    "@rollup/rollup-linux-loong64-gnu": ["@rollup/rollup-linux-loong64-gnu@4.60.4", "", { "os": "linux", "cpu": "none" }, "sha512-NxnomyxYerDh5n4iLrNa+sH+Z+U4BMEE46V2PgQ/hoB909i8gV1M5wPojWg9fk1jWpO3IQnOs20K4wyZuFLEFQ=="],
+
+    "@rollup/rollup-linux-loong64-musl": ["@rollup/rollup-linux-loong64-musl@4.60.4", "", { "os": "linux", "cpu": "none" }, "sha512-nbJnQ8a3z1mtmrwImCYhc6BGpThAyYVRQxw9uKSKG4wR6aAYno9sVjJ0zaZcW9BPJX1GbrDPf+SvdWjgTuDmnw=="],
+
+    "@rollup/rollup-linux-ppc64-gnu": ["@rollup/rollup-linux-ppc64-gnu@4.60.4", "", { "os": "linux", "cpu": "ppc64" }, "sha512-2EU6acNrQLd8tYvo/LXW535wupT3m6fo7HKo6lr7ktQoItxTyOL1ZCR/GfGCuXl2vR+zmfI6eRXkSemafv+iVg=="],
+
+    "@rollup/rollup-linux-ppc64-musl": ["@rollup/rollup-linux-ppc64-musl@4.60.4", "", { "os": "linux", "cpu": "ppc64" }, "sha512-WeBtoMuaMxiiIrO2IYP3xs6GMWkJP2C0EoT8beTLkUPmzV1i/UcOSVw1d5r9KBODtHKilG5yFxsGRnBbK3wJ4A=="],
+
+    "@rollup/rollup-linux-riscv64-gnu": ["@rollup/rollup-linux-riscv64-gnu@4.60.4", "", { "os": "linux", "cpu": "none" }, "sha512-FJHFfqpKUI3A10WrWKiFbBZ7yVbGT4q4B5o1qKFFojqpaYoh9LrQgqWCmmcxQzVSXYtyB5bzkXrYzlHTs21MYA=="],
+
+    "@rollup/rollup-linux-riscv64-musl": ["@rollup/rollup-linux-riscv64-musl@4.60.4", "", { "os": "linux", "cpu": "none" }, "sha512-mcEl6CUT5IAUmQf1m9FYSmVqCJlpQ8r8eyftFUHG8i9OhY7BkBXSUdnLH5DOf0wCOjcP9v/QO93zpmF1SptCCw=="],
+
+    "@rollup/rollup-linux-s390x-gnu": ["@rollup/rollup-linux-s390x-gnu@4.60.4", "", { "os": "linux", "cpu": "s390x" }, "sha512-ynt3JxVd2w2buzoKDWIyiV1pJW93xlQic1THVLXilz429oijRpSHivZAgp65KBu+cMcgf1eVVjdnTLvPxgCuoQ=="],
+
+    "@rollup/rollup-linux-x64-gnu": ["@rollup/rollup-linux-x64-gnu@4.60.4", "", { "os": "linux", "cpu": "x64" }, "sha512-Boiz5+MsaROEWDf+GGEwF8VMHGhlUoQMtIPjOgA5fv4osupqTVnJteQNKJwUcnUog2G55jYXH7KZFFiJe0TEzQ=="],
+
+    "@rollup/rollup-linux-x64-musl": ["@rollup/rollup-linux-x64-musl@4.60.4", "", { "os": "linux", "cpu": "x64" }, "sha512-+qfSY27qIrFfI/Hom04KYFw3GKZSGU4lXus51wsb5EuySfFlWRwjkKWoE9emgRw/ukoT4Udsj4W/+xxG8VbPKg=="],
+
+    "@rollup/rollup-openbsd-x64": ["@rollup/rollup-openbsd-x64@4.60.4", "", { "os": "openbsd", "cpu": "x64" }, "sha512-VpTfOPHgVXEBeeR8hZ2O0F3aSso+JDWqTWmTmzcQKted54IAdUVbxE+j/MVxUsKa8L20HJhv3vUezVPoquqWjA=="],
+
+    "@rollup/rollup-openharmony-arm64": ["@rollup/rollup-openharmony-arm64@4.60.4", "", { "os": "none", "cpu": "arm64" }, "sha512-IPOsh5aRYuLv/nkU51X10Bf75Bsf6+gZdx1X+QP5QM6lIJFHHqbHLG0uJn/hWthzo13UAc2umiUorqZy3axoZg=="],
+
+    "@rollup/rollup-win32-arm64-msvc": ["@rollup/rollup-win32-arm64-msvc@4.60.4", "", { "os": "win32", "cpu": "arm64" }, "sha512-4QzE9E81OohJ/HKzHhsqU+zcYYojVOXlFMs1DdyMT6qXl/niOH7AVElmmEdUNHHS/oRkc++d5k6Vy85zFs0DEw=="],
+
+    "@rollup/rollup-win32-ia32-msvc": ["@rollup/rollup-win32-ia32-msvc@4.60.4", "", { "os": "win32", "cpu": "ia32" }, "sha512-zTPgT1YuHHcd+Tmx7h8aml0FWFVelV5N54oHow9SLj+GfoDy/huQ+UV396N/C7KpMDMiPspRktzM1/0r1usYEA=="],
+
+    "@rollup/rollup-win32-x64-gnu": ["@rollup/rollup-win32-x64-gnu@4.60.4", "", { "os": "win32", "cpu": "x64" }, "sha512-DRS4G7mi9lJxqEDezIkKCaUIKCrLUUDCUaCsTPCi/rtqaC6D/jjwslMQyiDU50Ka0JKpeXeRBFBAXwArY52vBw=="],
+
+    "@rollup/rollup-win32-x64-msvc": ["@rollup/rollup-win32-x64-msvc@4.60.4", "", { "os": "win32", "cpu": "x64" }, "sha512-QVTUovf40zgTqlFVrKA1uXMVvU2QWEFWfAH8Wdc48IxLvrJMQVMBRjuQyUpzZCDkakImib9eVazbWlC6ksWtJw=="],
+
+    "@types/estree": ["@types/estree@1.0.9", "", {}, "sha512-GhdPgy1el4/ImP05X05Uw4cw2/M93BCUmnEvWZNStlCzEKME4Fkk+YpoA5OiHNQmoS7Cafb8Xa3Pya8m1Qrzeg=="],
+
+    "@types/node": ["@types/node@20.19.41", "", { "dependencies": { "undici-types": "~6.21.0" } }, "sha512-ECymXOukMnOoVkC2bb1Vc/w/836DXncOg5m8Xj1RH7xSHZJWNYY6Zh7EH477vcnD5egKNNfy2RpNOmuChhFPgQ=="],
+
+    "@vitest/expect": ["@vitest/expect@2.1.9", "", { "dependencies": { "@vitest/spy": "2.1.9", "@vitest/utils": "2.1.9", "chai": "^5.1.2", "tinyrainbow": "^1.2.0" } }, "sha512-UJCIkTBenHeKT1TTlKMJWy1laZewsRIzYighyYiJKZreqtdxSos/S1t+ktRMQWu2CKqaarrkeszJx1cgC5tGZw=="],
+
+    "@vitest/mocker": ["@vitest/mocker@2.1.9", "", { "dependencies": { "@vitest/spy": "2.1.9", "estree-walker": "^3.0.3", "magic-string": "^0.30.12" }, "peerDependencies": { "msw": "^2.4.9", "vite": "^5.0.0" }, "optionalPeers": ["msw", "vite"] }, "sha512-tVL6uJgoUdi6icpxmdrn5YNo3g3Dxv+IHJBr0GXHaEdTcw3F+cPKnsXFhli6nO+f/6SDKPHEK1UN+k+TQv0Ehg=="],
+
+    "@vitest/pretty-format": ["@vitest/pretty-format@2.1.9", "", { "dependencies": { "tinyrainbow": "^1.2.0" } }, "sha512-KhRIdGV2U9HOUzxfiHmY8IFHTdqtOhIzCpd8WRdJiE7D/HUcZVD0EgQCVjm+Q9gkUXWgBvMmTtZgIG48wq7sOQ=="],
+
+    "@vitest/runner": ["@vitest/runner@2.1.9", "", { "dependencies": { "@vitest/utils": "2.1.9", "pathe": "^1.1.2" } }, "sha512-ZXSSqTFIrzduD63btIfEyOmNcBmQvgOVsPNPe0jYtESiXkhd8u2erDLnMxmGrDCwHCCHE7hxwRDCT3pt0esT4g=="],
+
+    "@vitest/snapshot": ["@vitest/snapshot@2.1.9", "", { "dependencies": { "@vitest/pretty-format": "2.1.9", "magic-string": "^0.30.12", "pathe": "^1.1.2" } }, "sha512-oBO82rEjsxLNJincVhLhaxxZdEtV0EFHMK5Kmx5sJ6H9L183dHECjiefOAdnqpIgT5eZwT04PoggUnW88vOBNQ=="],
+
+    "@vitest/spy": ["@vitest/spy@2.1.9", "", { "dependencies": { "tinyspy": "^3.0.2" } }, "sha512-E1B35FwzXXTs9FHNK6bDszs7mtydNi5MIfUWpceJ8Xbfb1gBMscAnwLbEu+B44ed6W3XjL9/ehLPHR1fkf1KLQ=="],
+
+    "@vitest/utils": ["@vitest/utils@2.1.9", "", { "dependencies": { "@vitest/pretty-format": "2.1.9", "loupe": "^3.1.2", "tinyrainbow": "^1.2.0" } }, "sha512-v0psaMSkNJ3A2NMrUEHFRzJtDPFn+/VWZ5WxImB21T9fjucJRmS7xCS3ppEnARb9y11OAzaD+P2Ps+b+BGX5iQ=="],
+
+    "abort-controller": ["abort-controller@3.0.0", "", { "dependencies": { "event-target-shim": "^5.0.0" } }, "sha512-h8lQ8tacZYnR3vNQTgibj+tODHI5/+l06Au2Pcriv/Gmet0eaj4TwWH41sO9wnHDiQsEj19q0drzdWdeAHtweg=="],
+
+    "assertion-error": ["assertion-error@2.0.1", "", {}, "sha512-Izi8RQcffqCeNVgFigKli1ssklIbpHnCYc6AknXGYoB6grJqyeby7jv12JUQgmTAnIDnbck1uxksT4dzN3PWBA=="],
+
+    "cac": ["cac@6.7.14", "", {}, "sha512-b6Ilus+c3RrdDk+JhLKUAQfzzgLEPy6wcXqS7f/xe1EETvsDP6GORG7SFuOs6cID5YkqchW/LXZbX5bc8j7ZcQ=="],
+
+    "chai": ["chai@5.3.3", "", { "dependencies": { "assertion-error": "^2.0.1", "check-error": "^2.1.1", "deep-eql": "^5.0.1", "loupe": "^3.1.0", "pathval": "^2.0.0" } }, "sha512-4zNhdJD/iOjSH0A05ea+Ke6MU5mmpQcbQsSOkgdaUMJ9zTlDTD/GYlwohmIE2u0gaxHYiVHEn1Fw9mZ/ktJWgw=="],
+
+    "check-error": ["check-error@2.1.3", "", {}, "sha512-PAJdDJusoxnwm1VwW07VWwUN1sl7smmC3OKggvndJFadxxDRyFJBX/ggnu/KE4kQAB7a3Dp8f/YXC1FlUprWmA=="],
+
+    "debug": ["debug@4.4.3", "", { "dependencies": { "ms": "^2.1.3" } }, "sha512-RGwwWnwQvkVfavKVt22FGLw+xYSdzARwm0ru6DhTVA3umU5hZc28V3kO4stgYryrTlLpuvgI9GiijltAjNbcqA=="],
+
+    "deep-eql": ["deep-eql@5.0.2", "", {}, "sha512-h5k/5U50IJJFpzfL6nO9jaaumfjO/f2NjK/oYB2Djzm4p9L+3T9qWpZqZ2hAbLPuuYq9wrU08WQyBTL5GbPk5Q=="],
+
+    "es-module-lexer": ["es-module-lexer@1.7.0", "", {}, "sha512-jEQoCwk8hyb2AZziIOLhDqpm5+2ww5uIE6lkO/6jcOCusfk6LhMHpXXfBLXTZ7Ydyt0j4VoUQv6uGNYbdW+kBA=="],
+
+    "esbuild": ["esbuild@0.21.5", "", { "optionalDependencies": { "@esbuild/aix-ppc64": "0.21.5", "@esbuild/android-arm": "0.21.5", "@esbuild/android-arm64": "0.21.5", "@esbuild/android-x64": "0.21.5", "@esbuild/darwin-arm64": "0.21.5", "@esbuild/darwin-x64": "0.21.5", "@esbuild/freebsd-arm64": "0.21.5", "@esbuild/freebsd-x64": "0.21.5", "@esbuild/linux-arm": "0.21.5", "@esbuild/linux-arm64": "0.21.5", "@esbuild/linux-ia32": "0.21.5", "@esbuild/linux-loong64": "0.21.5", "@esbuild/linux-mips64el": "0.21.5", "@esbuild/linux-ppc64": "0.21.5", "@esbuild/linux-riscv64": "0.21.5", "@esbuild/linux-s390x": "0.21.5", "@esbuild/linux-x64": "0.21.5", "@esbuild/netbsd-x64": "0.21.5", "@esbuild/openbsd-x64": "0.21.5", "@esbuild/sunos-x64": "0.21.5", "@esbuild/win32-arm64": "0.21.5", "@esbuild/win32-ia32": "0.21.5", "@esbuild/win32-x64": "0.21.5" }, "bin": { "esbuild": "bin/esbuild" } }, "sha512-mg3OPMV4hXywwpoDxu3Qda5xCKQi+vCTZq8S9J/EpkhB2HzKXq4SNFZE3+NK93JYxc8VMSep+lOUSC/RVKaBqw=="],
+
+    "estree-walker": ["estree-walker@3.0.3", "", { "dependencies": { "@types/estree": "^1.0.0" } }, "sha512-7RUKfXgSMMkzt6ZuXmqapOurLGPPfgj6l9uRZ7lRGolvk0y2yocc35LdcxKC5PQZdn2DMqioAQ2NoWcrTKmm6g=="],
+
+    "event-target-shim": ["event-target-shim@5.0.1", "", {}, "sha512-i/2XbnSz/uxRCU6+NdVJgKWDTM427+MqYbkQzD321DuCQJUqOuJKIA0IM2+W2xtYHdKOmZ4dR6fExsd4SXL+WQ=="],
+
+    "expect-type": ["expect-type@1.3.0", "", {}, "sha512-knvyeauYhqjOYvQ66MznSMs83wmHrCycNEN6Ao+2AeYEfxUIkuiVxdEa1qlGEPK+We3n0THiDciYSsCcgW/DoA=="],
+
+    "fsevents": ["fsevents@2.3.3", "", { "os": "darwin" }, "sha512-5xoDfX+fL7faATnagmWPpbFtwh/R77WmMMqqHGS65C3vvB0YHrgF+B1YmZ3441tMj5n63k0212XNoJwzlhffQw=="],
+
+    "grammy": ["grammy@1.42.0", "", { "dependencies": { "@grammyjs/types": "3.26.0", "abort-controller": "^3.0.0", "debug": "^4.4.3", "node-fetch": "^2.7.0" } }, "sha512-1AdCge+AkjSdp2FwfICSFnVbl8Mq3KVHJDy+DgTI9+D6keJ0zWALPRKas5jv/8psiCzL4N2cEOcGW7O45Kn39g=="],
+
+    "loupe": ["loupe@3.2.1", "", {}, "sha512-CdzqowRJCeLU72bHvWqwRBBlLcMEtIvGrlvef74kMnV2AolS9Y8xUv1I0U/MNAWMhBlKIoyuEgoJ0t/bbwHbLQ=="],
+
+    "magic-string": ["magic-string@0.30.21", "", { "dependencies": { "@jridgewell/sourcemap-codec": "^1.5.5" } }, "sha512-vd2F4YUyEXKGcLHoq+TEyCjxueSeHnFxyyjNp80yg0XV4vUhnDer/lvvlqM/arB5bXQN5K2/3oinyCRyx8T2CQ=="],
+
+    "ms": ["ms@2.1.3", "", {}, "sha512-6FlzubTLZG3J2a/NVCAleEhjzq5oxgHyaCU9yYXvcLsvoVaHJq/s5xXI6/XXP6tz7R9xAOtHnSO/tXtF3WRTlA=="],
+
+    "nanoid": ["nanoid@3.3.12", "", { "bin": { "nanoid": "bin/nanoid.cjs" } }, "sha512-ZB9RH/39qpq5Vu6Y+NmUaFhQR6pp+M2Xt76XBnEwDaGcVAqhlvxrl3B2bKS5D3NH3QR76v3aSrKaF/Kiy7lEtQ=="],
+
+    "node-fetch": ["node-fetch@2.7.0", "", { "dependencies": { "whatwg-url": "^5.0.0" }, "peerDependencies": { "encoding": "^0.1.0" }, "optionalPeers": ["encoding"] }, "sha512-c4FRfUm/dbcWZ7U+1Wq0AwCyFL+3nt2bEw05wfxSz+DWpWsitgmSgYmy2dQdWyKC1694ELPqMs/YzUSNozLt8A=="],
+
+    "pathe": ["pathe@1.1.2", "", {}, "sha512-whLdWMYL2TwI08hn8/ZqAbrVemu0LNaNNJZX73O6qaIdCTfXutsLhMkjdENX0qhsQ9uIimo4/aQOmXkoon2nDQ=="],
+
+    "pathval": ["pathval@2.0.1", "", {}, "sha512-//nshmD55c46FuFw26xV/xFAaB5HF9Xdap7HJBBnrKdAd6/GxDBaNA1870O79+9ueg61cZLSVc+OaFlfmObYVQ=="],
+
+    "picocolors": ["picocolors@1.1.1", "", {}, "sha512-xceH2snhtb5M9liqDsmEw56le376mTZkEX/jEb/RxNFyegNul7eNslCXP9FDj/Lcu0X8KEyMceP2ntpaHrDEVA=="],
+
+    "postcss": ["postcss@8.5.14", "", { "dependencies": { "nanoid": "^3.3.11", "picocolors": "^1.1.1", "source-map-js": "^1.2.1" } }, "sha512-SoSL4+OSEtR99LHFZQiJLkT59C5B1amGO1NzTwj7TT1qCUgUO6hxOvzkOYxD+vMrXBM3XJIKzokoERdqQq/Zmg=="],
+
+    "rollup": ["rollup@4.60.4", "", { "dependencies": { "@types/estree": "1.0.8" }, "optionalDependencies": { "@rollup/rollup-android-arm-eabi": "4.60.4", "@rollup/rollup-android-arm64": "4.60.4", "@rollup/rollup-darwin-arm64": "4.60.4", "@rollup/rollup-darwin-x64": "4.60.4", "@rollup/rollup-freebsd-arm64": "4.60.4", "@rollup/rollup-freebsd-x64": "4.60.4", "@rollup/rollup-linux-arm-gnueabihf": "4.60.4", "@rollup/rollup-linux-arm-musleabihf": "4.60.4", "@rollup/rollup-linux-arm64-gnu": "4.60.4", "@rollup/rollup-linux-arm64-musl": "4.60.4", "@rollup/rollup-linux-loong64-gnu": "4.60.4", "@rollup/rollup-linux-loong64-musl": "4.60.4", "@rollup/rollup-linux-ppc64-gnu": "4.60.4", "@rollup/rollup-linux-ppc64-musl": "4.60.4", "@rollup/rollup-linux-riscv64-gnu": "4.60.4", "@rollup/rollup-linux-riscv64-musl": "4.60.4", "@rollup/rollup-linux-s390x-gnu": "4.60.4", "@rollup/rollup-linux-x64-gnu": "4.60.4", "@rollup/rollup-linux-x64-musl": "4.60.4", "@rollup/rollup-openbsd-x64": "4.60.4", "@rollup/rollup-openharmony-arm64": "4.60.4", "@rollup/rollup-win32-arm64-msvc": "4.60.4", "@rollup/rollup-win32-ia32-msvc": "4.60.4", "@rollup/rollup-win32-x64-gnu": "4.60.4", "@rollup/rollup-win32-x64-msvc": "4.60.4", "fsevents": "~2.3.2" }, "bin": { "rollup": "dist/bin/rollup" } }, "sha512-WHeFSbZYsPu3+bLoNRUuAO+wavNlocOPf3wSHTP7hcFKVnJeWsYlCDbr3mTS14FCizf9ccIxXA8sGL8zKeQN3g=="],
+
+    "siginfo": ["siginfo@2.0.0", "", {}, "sha512-ybx0WO1/8bSBLEWXZvEd7gMW3Sn3JFlW3TvX1nREbDLRNQNaeNN8WK0meBwPdAaOI7TtRRRJn/Es1zhrrCHu7g=="],
+
+    "source-map-js": ["source-map-js@1.2.1", "", {}, "sha512-UXWMKhLOwVKb728IUtQPXxfYU+usdybtUrK/8uGE8CQMvrhOpwvzDBwj0QhSL7MQc7vIsISBG8VQ8+IDQxpfQA=="],
+
+    "stackback": ["stackback@0.0.2", "", {}, "sha512-1XMJE5fQo1jGH6Y/7ebnwPOBEkIEnT4QF32d5R1+VXdXveM0IBMJt8zfaxX1P3QhVwrYe+576+jkANtSS2mBbw=="],
+
+    "std-env": ["std-env@3.10.0", "", {}, "sha512-5GS12FdOZNliM5mAOxFRg7Ir0pWz8MdpYm6AY6VPkGpbA7ZzmbzNcBJQ0GPvvyWgcY7QAhCgf9Uy89I03faLkg=="],
+
+    "tinybench": ["tinybench@2.9.0", "", {}, "sha512-0+DUvqWMValLmha6lr4kD8iAMK1HzV0/aKnCtWb9v9641TnP/MFb7Pc2bxoxQjTXAErryXVgUOfv2YqNllqGeg=="],
+
+    "tinyexec": ["tinyexec@0.3.2", "", {}, "sha512-KQQR9yN7R5+OSwaK0XQoj22pwHoTlgYqmUscPYoknOoWCWfj/5/ABTMRi69FrKU5ffPVh5QcFikpWJI/P1ocHA=="],
+
+    "tinypool": ["tinypool@1.1.1", "", {}, "sha512-Zba82s87IFq9A9XmjiX5uZA/ARWDrB03OHlq+Vw1fSdt0I+4/Kutwy8BP4Y/y/aORMo61FQ0vIb5j44vSo5Pkg=="],
+
+    "tinyrainbow": ["tinyrainbow@1.2.0", "", {}, "sha512-weEDEq7Z5eTHPDh4xjX789+fHfF+P8boiFB+0vbWzpbnbsEr/GRaohi/uMKxg8RZMXnl1ItAi/IUHWMsjDV7kQ=="],
+
+    "tinyspy": ["tinyspy@3.0.2", "", {}, "sha512-n1cw8k1k0x4pgA2+9XrOkFydTerNcJ1zWCO5Nn9scWHTD+5tp8dghT2x1uduQePZTZgd3Tupf+x9BxJjeJi77Q=="],
+
+    "tr46": ["tr46@0.0.3", "", {}, "sha512-N3WMsuqV66lT30CrXNbEjx4GEwlow3v6rr4mCcv6prnfwhS01rkgyFdjPNBYd9br7LpXV1+Emh01fHnq2Gdgrw=="],
+
+    "typescript": ["typescript@5.9.3", "", { "bin": { "tsc": "bin/tsc", "tsserver": "bin/tsserver" } }, "sha512-jl1vZzPDinLr9eUt3J/t7V6FgNEw9QjvBPdysz9KfQDD41fQrC2Y4vKQdiaUpFT4bXlb1RHhLpp8wtm6M5TgSw=="],
+
+    "undici-types": ["undici-types@6.21.0", "", {}, "sha512-iwDZqg0QAGrg9Rav5H4n0M64c3mkR59cJ6wQp+7C4nI0gsmExaedaYLNO44eT4AtBBwjbTiGPMlt2Md0T9H9JQ=="],
+
+    "vite": ["vite@5.4.21", "", { "dependencies": { "esbuild": "^0.21.3", "postcss": "^8.4.43", "rollup": "^4.20.0" }, "optionalDependencies": { "fsevents": "~2.3.3" }, "peerDependencies": { "@types/node": "^18.0.0 || >=20.0.0", "less": "*", "lightningcss": "^1.21.0", "sass": "*", "sass-embedded": "*", "stylus": "*", "sugarss": "*", "terser": "^5.4.0" }, "optionalPeers": ["@types/node", "less", "lightningcss", "sass", "sass-embedded", "stylus", "sugarss", "terser"], "bin": { "vite": "bin/vite.js" } }, "sha512-o5a9xKjbtuhY6Bi5S3+HvbRERmouabWbyUcpXXUA1u+GNUKoROi9byOJ8M0nHbHYHkYICiMlqxkg1KkYmm25Sw=="],
+
+    "vite-node": ["vite-node@2.1.9", "", { "dependencies": { "cac": "^6.7.14", "debug": "^4.3.7", "es-module-lexer": "^1.5.4", "pathe": "^1.1.2", "vite": "^5.0.0" }, "bin": { "vite-node": "vite-node.mjs" } }, "sha512-AM9aQ/IPrW/6ENLQg3AGY4K1N2TGZdR5e4gu/MmmR2xR3Ll1+dib+nook92g4TV3PXVyeyxdWwtaCAiUL0hMxA=="],
+
+    "vitest": ["vitest@2.1.9", "", { "dependencies": { "@vitest/expect": "2.1.9", "@vitest/mocker": "2.1.9", "@vitest/pretty-format": "^2.1.9", "@vitest/runner": "2.1.9", "@vitest/snapshot": "2.1.9", "@vitest/spy": "2.1.9", "@vitest/utils": "2.1.9", "chai": "^5.1.2", "debug": "^4.3.7", "expect-type": "^1.1.0", "magic-string": "^0.30.12", "pathe": "^1.1.2", "std-env": "^3.8.0", "tinybench": "^2.9.0", "tinyexec": "^0.3.1", "tinypool": "^1.0.1", "tinyrainbow": "^1.2.0", "vite": "^5.0.0", "vite-node": "2.1.9", "why-is-node-running": "^2.3.0" }, "peerDependencies": { "@edge-runtime/vm": "*", "@types/node": "^18.0.0 || >=20.0.0", "@vitest/browser": "2.1.9", "@vitest/ui": "2.1.9", "happy-dom": "*", "jsdom": "*" }, "optionalPeers": ["@edge-runtime/vm", "@types/node", "@vitest/browser", "@vitest/ui", "happy-dom", "jsdom"], "bin": { "vitest": "vitest.mjs" } }, "sha512-MSmPM9REYqDGBI8439mA4mWhV5sKmDlBKWIYbA3lRb2PTHACE0mgKwA8yQ2xq9vxDTuk4iPrECBAEW2aoFXY0Q=="],
+
+    "webidl-conversions": ["webidl-conversions@3.0.1", "", {}, "sha512-2JAn3z8AR6rjK8Sm8orRC0h/bcl/DqL7tRPdGZ4I1CjdF+EaMLmYxBHyXuKL849eucPFhvBoxMsflfOb8kxaeQ=="],
+
+    "whatwg-url": ["whatwg-url@5.0.0", "", { "dependencies": { "tr46": "~0.0.3", "webidl-conversions": "^3.0.0" } }, "sha512-saE57nupxk6v3HY35+jzBwYa0rKSy0XR8JSxZPwgLr7ys0IBzhGviA1/TUGJLmSVqs8pb9AnvICXEuOHLprYTw=="],
+
+    "why-is-node-running": ["why-is-node-running@2.3.0", "", { "dependencies": { "siginfo": "^2.0.0", "stackback": "0.0.2" }, "bin": { "why-is-node-running": "cli.js" } }, "sha512-hUrmaWBdVDcxvYqnyh09zunKzROWjbZTiNy8dBEjkS7ehEDQibXJ7XvlmtbwuTclUiIyN+CyXQD4Vmko8fNm8w=="],
+
+    "rollup/@types/estree": ["@types/estree@1.0.8", "", {}, "sha512-dWHzHa2WqEXI/O1E9OjrocMTKJl2mSrEolh1Iomrv6U+JuNwaHXsXx9bLu5gG7BUWFIN0skIQJQ/L1rIex4X6w=="],
+  }
+}
diff --git a/02-integration-clients/index.ts b/02-integration-clients/index.ts
new file mode 100644
index 0000000..ae1a0b8
--- /dev/null
+++ b/02-integration-clients/index.ts
@@ -0,0 +1,8 @@
+export * from './src/errors'
+
+// TODO (section-03): export * from './src/telegram/TelegramClient'
+// TODO (section-03): export * from './src/telegram/types'
+// TODO (section-04): export * from './src/jira/JiraClient'
+// TODO (section-04): export * from './src/jira/types'
+// TODO (section-05): export * from './src/claude/ClaudeClient'
+// TODO (section-05): export * from './src/claude/types'
diff --git a/02-integration-clients/package.json b/02-integration-clients/package.json
new file mode 100644
index 0000000..ed78a91
--- /dev/null
+++ b/02-integration-clients/package.json
@@ -0,0 +1,17 @@
+{
+  "name": "02-integration-clients",
+  "version": "0.0.1",
+  "type": "module",
+  "scripts": {
+    "test": "vitest run",
+    "test:watch": "vitest"
+  },
+  "dependencies": {
+    "grammy": "^1.36.0"
+  },
+  "devDependencies": {
+    "vitest": "^2.1.0",
+    "typescript": "^5.8.0",
+    "@types/node": "^20.17.0"
+  }
+}
diff --git a/02-integration-clients/src/errors.ts b/02-integration-clients/src/errors.ts
new file mode 100644
index 0000000..b8c80ed
--- /dev/null
+++ b/02-integration-clients/src/errors.ts
@@ -0,0 +1,87 @@
+export class JiraAuthError extends Error {
+  readonly type = 'JIRA_AUTH' as const
+  constructor(message = 'Jira authentication failed') {
+    super(message)
+    this.name = 'JiraAuthError'
+  }
+}
+
+export class JiraPermissionError extends Error {
+  readonly type = 'JIRA_PERMISSION' as const
+  constructor(message = 'Jira permission denied') {
+    super(message)
+    this.name = 'JiraPermissionError'
+  }
+}
+
+export class JiraNotFoundError extends Error {
+  readonly type = 'JIRA_NOT_FOUND' as const
+  readonly issueKey: string
+  constructor(issueKey: string, message?: string) {
+    super(message ?? `Issue ${issueKey} not found`)
+    this.name = 'JiraNotFoundError'
+    this.issueKey = issueKey
+  }
+}
+
+export class JiraRateLimitError extends Error {
+  readonly type = 'JIRA_RATE_LIMIT' as const
+  readonly retryAfter?: number
+  constructor(retryAfter?: number, message = 'Jira rate limit exceeded') {
+    super(message)
+    this.name = 'JiraRateLimitError'
+    this.retryAfter = retryAfter
+  }
+}
+
+export class JiraServerError extends Error {
+  readonly type = 'JIRA_SERVER' as const
+  readonly status: number
+  constructor(status: number, message?: string) {
+    super(message ?? `Jira server error: ${status}`)
+    this.name = 'JiraServerError'
+    this.status = status
+  }
+}
+
+export class JiraTimeoutError extends Error {
+  readonly type = 'JIRA_TIMEOUT' as const
+  constructor(message = 'Jira request timed out') {
+    super(message)
+    this.name = 'JiraTimeoutError'
+  }
+}
+
+export class InvalidTransitionError extends Error {
+  readonly type = 'INVALID_TRANSITION' as const
+  readonly attempted: string
+  readonly available: string[]
+  constructor(attempted: string, available: string[], message?: string) {
+    super(message ?? `Invalid transition '${attempted}'. Available: ${available.join(', ')}`)
+    this.name = 'InvalidTransitionError'
+    this.attempted = attempted
+    this.available = available
+  }
+}
+
+export class ClaudeTimeoutError extends Error {
+  readonly type = 'CLAUDE_TIMEOUT' as const
+  readonly timeoutMs: number
+  constructor(timeoutMs: number, message?: string) {
+    super(message ?? `Claude timed out after ${timeoutMs}ms`)
+    this.name = 'ClaudeTimeoutError'
+    this.timeoutMs = timeoutMs
+  }
+}
+
+export class ClaudeExitError extends Error {
+  readonly type = 'CLAUDE_EXIT' as const
+  readonly exitCode: number
+  readonly stderr: string
+  constructor(exitCode: number, stderr: string, message?: string) {
+    super(message ?? `Claude exited with code ${exitCode}`)
+    this.name = 'ClaudeExitError'
+    this.exitCode = exitCode
+    this.stderr = stderr
+  }
+}
diff --git a/02-integration-clients/src/index.ts b/02-integration-clients/src/index.ts
deleted file mode 100644
index 5f8fe81..0000000
--- a/02-integration-clients/src/index.ts
+++ /dev/null
@@ -1,4 +0,0 @@
-export async function startPolling(_signal: AbortSignal): Promise<void> {
-  // placeholder — implemented in 02-integration-clients section
-  await new Promise<void>(() => {})
-}
diff --git a/02-integration-clients/tests/errors.test.ts b/02-integration-clients/tests/errors.test.ts
new file mode 100644
index 0000000..c7aea6b
--- /dev/null
+++ b/02-integration-clients/tests/errors.test.ts
@@ -0,0 +1,148 @@
+import { describe, it, expect } from 'vitest'
+import {
+  JiraAuthError,
+  JiraPermissionError,
+  JiraNotFoundError,
+  JiraRateLimitError,
+  JiraServerError,
+  JiraTimeoutError,
+  InvalidTransitionError,
+  ClaudeTimeoutError,
+  ClaudeExitError,
+} from '../src/errors'
+
+describe('JiraAuthError', () => {
+  it('is instanceof Error with correct type', () => {
+    const err = new JiraAuthError()
+    expect(err).toBeInstanceOf(Error)
+    expect(err.type).toBe('JIRA_AUTH')
+    expect(err.message).toBe('Jira authentication failed')
+    expect(err.name).toBe('JiraAuthError')
+  })
+
+  it('accepts custom message', () => {
+    const err = new JiraAuthError('custom')
+    expect(err.message).toBe('custom')
+  })
+})
+
+describe('JiraPermissionError', () => {
+  it('is instanceof Error with correct type', () => {
+    const err = new JiraPermissionError()
+    expect(err).toBeInstanceOf(Error)
+    expect(err.type).toBe('JIRA_PERMISSION')
+    expect(err.name).toBe('JiraPermissionError')
+  })
+})
+
+describe('JiraNotFoundError', () => {
+  it('carries issueKey', () => {
+    const err = new JiraNotFoundError('PROJ-123')
+    expect(err).toBeInstanceOf(Error)
+    expect(err.type).toBe('JIRA_NOT_FOUND')
+    expect(err.issueKey).toBe('PROJ-123')
+    expect(err.message).toBe('Issue PROJ-123 not found')
+    expect(err.name).toBe('JiraNotFoundError')
+  })
+
+  it('accepts custom message', () => {
+    const err = new JiraNotFoundError('PROJ-1', 'gone')
+    expect(err.message).toBe('gone')
+  })
+})
+
+describe('JiraRateLimitError', () => {
+  it('carries optional retryAfter', () => {
+    const withRetry = new JiraRateLimitError(30)
+    expect(withRetry).toBeInstanceOf(Error)
+    expect(withRetry.type).toBe('JIRA_RATE_LIMIT')
+    expect(withRetry.retryAfter).toBe(30)
+
+    const withoutRetry = new JiraRateLimitError()
+    expect(withoutRetry.retryAfter).toBeUndefined()
+  })
+})
+
+describe('JiraServerError', () => {
+  it('carries status code', () => {
+    const err = new JiraServerError(500)
+    expect(err).toBeInstanceOf(Error)
+    expect(err.type).toBe('JIRA_SERVER')
+    expect(err.status).toBe(500)
+    expect(err.name).toBe('JiraServerError')
+  })
+})
+
+describe('JiraTimeoutError', () => {
+  it('is instanceof Error with correct type', () => {
+    const err = new JiraTimeoutError()
+    expect(err).toBeInstanceOf(Error)
+    expect(err.type).toBe('JIRA_TIMEOUT')
+    expect(err.message).toBe('Jira request timed out')
+  })
+})
+
+describe('InvalidTransitionError', () => {
+  it('carries attempted and available[]', () => {
+    const err = new InvalidTransitionError('Close', ['Resolve', 'Reopen'])
+    expect(err).toBeInstanceOf(Error)
+    expect(err.type).toBe('INVALID_TRANSITION')
+    expect(err.attempted).toBe('Close')
+    expect(err.available).toEqual(['Resolve', 'Reopen'])
+    expect(err.name).toBe('InvalidTransitionError')
+  })
+})
+
+describe('ClaudeTimeoutError', () => {
+  it('carries timeoutMs', () => {
+    const err = new ClaudeTimeoutError(5000)
+    expect(err).toBeInstanceOf(Error)
+    expect(err.type).toBe('CLAUDE_TIMEOUT')
+    expect(err.timeoutMs).toBe(5000)
+    expect(err.name).toBe('ClaudeTimeoutError')
+  })
+})
+
+describe('ClaudeExitError', () => {
+  it('carries exitCode and stderr', () => {
+    const err = new ClaudeExitError(1, 'error output')
+    expect(err).toBeInstanceOf(Error)
+    expect(err.type).toBe('CLAUDE_EXIT')
+    expect(err.exitCode).toBe(1)
+    expect(err.stderr).toBe('error output')
+    expect(err.name).toBe('ClaudeExitError')
+  })
+})
+
+describe('type discriminant switch', () => {
+  it('all 9 types are uniquely distinguishable', () => {
+    const errors = [
+      new JiraAuthError(),
+      new JiraPermissionError(),
+      new JiraNotFoundError('X-1'),
+      new JiraRateLimitError(),
+      new JiraServerError(503),
+      new JiraTimeoutError(),
+      new InvalidTransitionError('a', []),
+      new ClaudeTimeoutError(1000),
+      new ClaudeExitError(2, ''),
+    ]
+
+    const seen = new Set<string>()
+    for (const err of errors) {
+      switch (err.type) {
+        case 'JIRA_AUTH': seen.add('JIRA_AUTH'); break
+        case 'JIRA_PERMISSION': seen.add('JIRA_PERMISSION'); break
+        case 'JIRA_NOT_FOUND': seen.add('JIRA_NOT_FOUND'); break
+        case 'JIRA_RATE_LIMIT': seen.add('JIRA_RATE_LIMIT'); break
+        case 'JIRA_SERVER': seen.add('JIRA_SERVER'); break
+        case 'JIRA_TIMEOUT': seen.add('JIRA_TIMEOUT'); break
+        case 'INVALID_TRANSITION': seen.add('INVALID_TRANSITION'); break
+        case 'CLAUDE_TIMEOUT': seen.add('CLAUDE_TIMEOUT'); break
+        case 'CLAUDE_EXIT': seen.add('CLAUDE_EXIT'); break
+      }
+    }
+
+    expect(seen.size).toBe(9)
+  })
+})
diff --git a/02-integration-clients/tsconfig.json b/02-integration-clients/tsconfig.json
new file mode 100644
index 0000000..ce4ac99
--- /dev/null
+++ b/02-integration-clients/tsconfig.json
@@ -0,0 +1,13 @@
+{
+  "compilerOptions": {
+    "target": "ESNext",
+    "module": "ESNext",
+    "moduleResolution": "bundler",
+    "strict": true,
+    "outDir": "dist",
+    "rootDir": ".",
+    "skipLibCheck": true,
+    "types": ["node"]
+  },
+  "include": ["src/**/*", "tests/**/*", "index.ts"]
+}
diff --git a/02-integration-clients/vitest.config.ts b/02-integration-clients/vitest.config.ts
new file mode 100644
index 0000000..0e49577
--- /dev/null
+++ b/02-integration-clients/vitest.config.ts
@@ -0,0 +1,8 @@
+import { defineConfig } from 'vitest/config'
+
+export default defineConfig({
+  test: {
+    globals: false,
+    environment: 'node',
+  },
+})
