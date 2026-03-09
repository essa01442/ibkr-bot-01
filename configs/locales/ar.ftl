# Robust Penny Scalper v7.0 — Arabic Locale
# All UI text, reject reasons, and alert codes in Arabic

## Reject Reasons (§2.1)
reject-blocklist = رمز محظور
reject-corporate-action-block = إجراء مؤسسي — محظور
reject-price-range = خارج نطاق السعر المستهدف
reject-liquidity = سيولة غير كافية
reject-regime = الوضع السوقي غير مناسب (ليس Normal)
reject-daily-context = لا يوجد محفز اليوم
reject-mtf-veto = تعارض مع الاتجاه متعدد الأطر
reject-anti-chase = ارتفاع سريع — مطاردة محتملة
reject-guard-spread = فارق السعر واسع جداً
reject-guard-imbalance = خلل في التوازن الشرائي/البيعي
reject-guard-stale = اقتباسات قديمة / بيانات معلقة
reject-guard-slippage = انزلاق متوقع مرتفع
reject-guard-l2-vacuum = عمق دفتر الأوامر فارغ
reject-guard-flicker = تذبذب مفرط في الاقتباسات
reject-tape-score-low = درجة الشريط أقل من العتبة
reject-net-negative = العائد الصافي المتوقع سالب
reject-exposure = تجاوز حدود التعرض أو الارتباط
reject-tape-reversal = انعكاس مفاجئ في الشريط
reject-monitor-only = النظام في وضع المراقبة فقط
reject-max-daily-loss = تم الوصول للحد اليومي للخسارة
reject-pdt-violation = انتهاك قاعدة PDT / التسوية

## Regime States (§11)
regime-normal = طبيعي
regime-caution = تحذير
regime-risk-off = إيقاف التداول

## OMS States (§19)
oms-idle = خامل
oms-active = نشط
oms-investigate = تحقيق — لا أوامر جديدة

## Data Quality
data-quality-ok = البيانات سليمة
data-quality-degraded = البيانات متدهورة — مراقبة فقط

## Cold Start States (§12)
cold-start-cold = بدء بارد
cold-start-warm = بدء دافئ
cold-start-full = نشط كامل

## Session States (§25)
session-closed = السوق مغلق
session-pre-market = ما قبل الجلسة
session-open-volatility = نافذة افتتاح متقلبة — انتظار
session-trading = جلسة التداول
session-close-volatility = نافذة إغلاق — لا دخول جديد
session-after-hours = ما بعد الجلسة

## Alerts (§24.4)
alert-data-api-down = تنبيه: انقطع الاتصال بـ IBKR
alert-heartbeat-missing = تنبيه: لا نبض — { $duration_secs } ثانية
alert-sla-breach = تنبيه: تجاوز SLA — P95 = { $p95_micros } µs
alert-daily-loss-limit = تنبيه: تم الوصول للحد اليومي للخسارة ({ $loss_usd }$)
alert-loss-ladder-activated = تنبيه: تفعّل سلم الخسائر — المستوى { $level }
alert-order-anomaly = تنبيه: أمر غير طبيعي — رقم { $order_id }
alert-ibkr-subs-high = تنبيه: اشتراكات IBKR { $current }/{ $limit }
alert-mtf-reject-high = تنبيه: نسبة رفض MTF مرتفعة ({ $rate_pct }%)

## Guard Names (§14)
guard-spread = حارس السبريد
guard-imbalance = حارس التوازن
guard-stale = حارس الاقتباسات القديمة
guard-slippage = حارس الانزلاق
guard-l2-vacuum = حارس عمق الدفتر
guard-flicker = حارس التذبذب

## Loss Attribution (§24.6)
loss-entry-model = نموذج الدخول
loss-context = السياق اليومي/متعدد الأطر
loss-guards = الحراس الهيكليون
loss-execution = التنفيذ والانزلاق
loss-risk = إدارة المخاطر
loss-data = جودة البيانات

## Exit Reasons (§19.4)
exit-target = الهدف المحقق
exit-stop = وقف الخسارة
exit-manual = خروج يدوي
exit-luld-halt = طارئ — وقف التداول
exit-regime-change = تغير الوضع السوقي
exit-tape-reversal = انعكاس الشريط
exit-session-close = نهاية الجلسة
exit-unknown = غير محدد