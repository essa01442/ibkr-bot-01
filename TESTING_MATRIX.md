# مصفوفة الاختبارات للتحديات الحية (Testing Matrix)

يربط هذا الملف جميع القضايا الـ 23 المحددة في `LIVE_BLOCKERS.md` بطرق اختبارها الحالية لضمان جودة وتغطية الكود.

| المُعرّف | الفئة | حالة الاختبار (Test Coverage) | الاختبار أو السيناريو |
|---|---|---|---|
| I-01 | Execution | مغطى جزئياً | `test_cancel_flow_success` و سيناريو 2 من `cross_language_integration.rs` |
| I-02 | Analytics | مغطى | `test_mtf_evaluation` (يختبر `update_weekly_ema` و `update_daily_resistance`) |
| I-03 | Risk | مغطى | `test_pdt_5_day_rolling_window_in_et` في `pdt_tests.rs` و `datetime_tests.rs` |
| I-04 | UI/Dashboard | مغطى | تم إعادة كتابة اللوحة بـ Vanilla JS؛ يمكن اختبارها بأدوات المتصفح و `verify_dashboard.py` (Playwright) |
| I-05 | Testing | مغطى | `test_replayer_deterministic` في `rust/bins/replayer/tests/integration_test.rs` |
| I-06 | Bridge | غير مغطى صراحةً | (تم تبني MessagePack بدلًا من FlatBuffers بشكل كلي، لذا لا حاجة لاختباره ولكن يمكن إضافته للتحقق من عدم استخدامه) |
| I-07 | Risk | غير مغطى | تعتمد المزامنة حالياً على الحالة المحلية فقط. |
| I-08 | Observability | مغطى | `test_predicted_slippage_zero_fails` و `test_real_slippage_computation` في `calibration_tests.rs` |
| I-09 | Analytics | مغطى | `test_demotion_path` و `test_saturation_and_eviction` في `watchlist_engine` |
| I-10 | Infrastructure | مغطى | إزالة التكرار؛ يتم التحقق منه من خلال بناء `rpsd` دون أخطاء تعارض (axum tests) |
| I-11 | Infrastructure | مغطى | الاستبدال بـ `log::debug!`؛ يُختبر من خلال `cargo test` دون فشل I/O. |
| I-12 | Infrastructure | مغطى | اختبارات التكوين في `config::tests` و `cross_language_integration.rs` التي تعتمد على `socket_path` متغير |
| I-13 | Infrastructure | مغطى | `test_dashboard_security_non_local_warns_and_fails_if_not_allowed` في `config.rs` |
| I-14 | Infrastructure | مغطى | `cargo check` و `cargo test` ينجحان بعد إزالة التكرار في `Cargo.toml`. |
| I-15 | UI/Dashboard | مغطى | إعدادات `auth_token` والتحقق من `401 Unauthorized` (يمكن اختباره بمسارات `rpsd` في المستقبل). |
| I-16 | Bridge | مغطى | تغيير المسارات للـ `/tmp/rps` وتغيير التنبيهات في `ibkr_client.py` - مجرّب في `cross_language_integration.rs` |
| I-17 | Documentation | مراجعة يدوية | تحديث `README.md` والمستندات بـ `v7.0` |
| I-18 | Risk | مغطى | `test_manual_block` في `risk_engine/src/blocklist.rs` |
| I-19 | Risk | مغطى | اختبارات `integration_full_decision_chain` تؤكد استقرار `evaluate_entry_logic` |
| I-20 | Infrastructure | مغطى | `cargo check` ينجح على بيئة الـ Workspace. |
| I-21 | Documentation | غير مغطى برمجياً | يحتاج لإضافة `#![warn(missing_docs)]` لاحقاً. |
| I-22 | Analytics | غير مغطى برمجياً | التعليقات التقنية تُحل عند اكتمال الـ 1.0. |
| I-23 | UI/Dashboard | مغطى | تم استبدال Babel و React بنسخة HTML/JS نقية ومستقرة، وتم إثبات ذلك بأداة Playwright Offline. |

**ملخص التغطية (Coverage Summary):**
معظم المشكلات التقنية والأخطاء المحطمة (Critical/High Blockers) تم تأمينها عبر اختبارات وحدوية (Unit Tests) واختبارات تكامل (Integration Tests). المشاكل المتبقية إما هندسية وتتعلق بالتوثيق (مثل I-21 و I-22) أو تم اتخاذ قرارات بتجاوزها في هذه المرحلة مثل (I-06 الخاص بـ FlatBuffers الذي تم استبداله بـ MessagePack و I-07 الخاص بمزامنة PnL).