# هندسة تدفق بيانات MTF (Multi-Timeframe Wiring)

## نظرة عامة
تصف هذه الوثيقة التعديلات والتدفق المعماري الخاص بربط محرك `MtfEngine` بمصادر البيانات الحقيقية من داخل `SlowLoop`، ومعالجة مخاطر البيانات القديمة (Stale Data) والبيانات الصفرية (Zero-Default).

## التدفق المعماري
1. **الاستقبال:** يستقبل `SlowLoop` أحداث `EventKind::Snapshot` التي تم توسيعها مؤخراً لتشمل `weekly_ema` و `daily_resistance`.
2. **التحديث:** يقوم `SlowLoop` بالبحث عن محرك `MtfEngine` الخاص بالرمز (أو إنشائه) واستدعاء دوال:
   - `update_weekly_ema(snap.weekly_ema, event.ts_src)`
   - `update_daily_resistance(snap.daily_resistance, event.ts_src)`
3. **التمرير للـ Watchlist:** يتم إعادة تقييم محرك MTF فوراً عبر دالة `evaluate(event.ts_src)` ويمرر الناتج النهائي إلى الـ `Watchlist` للوصول إلى محرك التداول السريع `FastLoop`.

## معالجة البيانات القديمة (Stale-data Detection)
- تم إضافة مهلة زمنية للتهيئة `stale_data_threshold_ms` ضمن قسم `[mtf]` في ملف `config.toml` (الافتراضي 3,600,000 مللي ثانية، أي ساعة واحدة).
- تقوم الدالة `evaluate(current_ts)` بالتحقق من الفارق الزمني بين `current_ts` وآخر وقت تحديث (`last_weekly_ema_ts`, `last_daily_res_ts`, إلخ).
- إذا تبين تجاوز المهلة لأي معامل، تقوم الدالة بإجبار شرط التأكيد الخاص به إلى **Neutral (False)**، وتسجل تحذيراً (`log::warn!`) موضحاً الرمز ونوع البيانات المفقودة، لتجنب اتخاذ قرارات بناءً على بيئة سوق متغيرة.

## حظر القيم الصفرية (Zero-Default Prevention)
- في حال كان حقل الإدخال لـ `EMA` أو الـ `Resistance` يساوي `0.0` (وهو السلوك الافتراضي عند عدم التوفر)، تقوم الدوال `update_weekly_ema` و `update_daily_resistance` بتجاهل التحديث كلياً لضمان عدم تمرير قيم صفرية.
- إن لم تتوفر القيمة نهائياً (لم تُحدث)، فالسلوك الافتراضي مبرمج لرفض الشرط (False) لأن المقاومة الابتدائية هي `f64::MAX` و `EMA` الابتدائي `0.0`.

## الاختبارات (Tests Verification)
تم إضافة 3 اختبارات تكاملية ضمن `mtf_engine`:
1. `test_mtf_evaluation`: يثبت تفاعل القيم إيجاباً مع البيانات الجديدة، وتجاوز البوابات الأربع.
2. `test_mtf_stale_data`: يثبت أن استدعاء `evaluate()` بعد انقضاء مدة `stale_data_threshold_ms` يُسقط جميع البوابات ويعيد `mtf_pass = false` بصورة قطعية.
3. `test_zero_default_prevention`: يثبت استحالة تحديث الحالة بقيم صفرية `0.0` وأنها تُعالج دائماً برفض التأكيد (Neutral).
