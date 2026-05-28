'use strict';

// ── Config ───────────────────────────────────────────────────────────────────
const PARAMS = {
    bearing_vibration_x: {
        label: 'Вибрация X', unit: 'mm/s',
        warn: 4.5, crit: 7.0, min: 0, max: 10,
        gaugeId: 'gauge_bearing_vibration_x',
        sparkId:  'spark_bearing_vibration_x',
        trendId:  'trend_bearing_vibration_x',
        color: '#00d4ff',
    },
    bearing_temperature: {
        label: 'Температура', unit: '°C',
        warn: 75, crit: 85, min: 50, max: 100,
        gaugeId: 'gauge_bearing_temperature',
        sparkId:  'spark_bearing_temperature',
        trendId:  'trend_bearing_temperature',
        color: '#ff9800',
    },
    motor_current: {
        label: 'Ток двигателя', unit: 'A',
        warn: 12, crit: 15, min: 5, max: 20,
        gaugeId: 'gauge_motor_current',
        sparkId:  'spark_motor_current',
        trendId:  'trend_motor_current',
        color: '#b388ff',
    },
};

const MAX_HISTORY = 60;
const MAX_SPARK   = 30;
const MAX_LOG     = 200;

// ── State ─────────────────────────────────────────────────────────────────────
// Для каждого параметра храним последнее agg-значение и последнее raw-значение раздельно
const state = {};
Object.keys(PARAMS).forEach(p => {
    state[p] = {
        aggValue:   null,   // последнее сглаженное значение (от процессора)
        rawValue:   null,   // последнее сырое значение
        trendX: [], trendY: [],
        sparkY: [],
        lastStatus: null,
    };
});

// ── ECharts instances ─────────────────────────────────────────────────────────
const gaugeCharts = {};
const trendCharts = {};
const sparkCharts = {};

// ── Classify ──────────────────────────────────────────────────────────────────
function classify(param, value) {
    const { warn, crit } = PARAMS[param];
    if (value >= crit) return 'critical';
    if (value >= warn) return 'warning';
    return 'normal';
}

// ── Gauge color zones: нормальная зона зелёная, предупреждение жёлтое, критическая красная ──
function gaugeZones(cfg) {
    // ECharts gauge: массив [до_какой_доли, цвет], слева направо по дуге
    const wf = (cfg.warn - cfg.min) / (cfg.max - cfg.min);
    const cf = (cfg.crit - cfg.min) / (cfg.max - cfg.min);
    return [
        [wf, '#00e676'],  // 0..warn  → зелёный
        [cf, '#ffb300'],  // warn..crit → жёлтый
        [1,  '#ff3d3d'],  // crit..max  → красный
    ];
}

// ── Pointer color: явно задаём цвет стрелки по значению (не 'auto') ───────────
function pointerColor(param, value) {
    const s = classify(param, value);
    return s === 'critical' ? '#ff3d3d' : s === 'warning' ? '#ffb300' : '#00e676';
}

// ── Init gauge ────────────────────────────────────────────────────────────────
function initGauge(param) {
    const cfg = PARAMS[param];
    const el = document.getElementById(cfg.gaugeId);
    if (!el) return;
    const chart = echarts.init(el, null, { renderer: 'svg' });
    chart.setOption({
        series: [{
            type: 'gauge',
            startAngle: 210, endAngle: -30,
            min: cfg.min, max: cfg.max,
            splitNumber: 5,
            radius: '92%',
            center: ['50%', '60%'],
            axisLine: {
                lineStyle: {
                    width: 14,
                    color: gaugeZones(cfg),
                },
            },
            axisTick:  { show: false },
            splitLine: {
                distance: -18, length: 14,
                lineStyle: { color: '#1e3040', width: 2 },
            },
            axisLabel: {
                distance: -28, fontSize: 10,
                color: '#4a6a7a',
                fontFamily: "'Share Tech Mono', monospace",
            },
            pointer: {
                length: '58%', width: 5,
                itemStyle: { color: '#00e676' },  // начальный цвет, обновляется при данных
            },
            anchor: {
                show: true, size: 12,
                itemStyle: { color: '#1e3040', borderColor: '#2a4a60', borderWidth: 2 },
            },
            detail: { show: false },
            data: [{ value: cfg.min }],
        }],
        backgroundColor: 'transparent',
    });
    gaugeCharts[param] = chart;
}

// ── Init trend ────────────────────────────────────────────────────────────────
function initTrend(param) {
    const cfg = PARAMS[param];
    const el = document.getElementById(cfg.trendId);
    if (!el) return;
    const chart = echarts.init(el, null, { renderer: 'svg' });
    chart.setOption({
        backgroundColor: 'transparent',
        grid: { top: 12, right: 12, bottom: 28, left: 44 },
        tooltip: {
            trigger: 'axis',
            backgroundColor: '#0d1318',
            borderColor: '#1e3040',
            borderWidth: 1,
            textStyle: { color: '#c8dde8', fontFamily: "'Share Tech Mono', monospace", fontSize: 11 },
            formatter: p => `<span style="color:${cfg.color}">${p[0].axisValue}</span><br/>${Number(p[0].value).toFixed(3)} ${cfg.unit}`,
        },
        xAxis: {
            type: 'category', data: [],
            axisLine: { lineStyle: { color: '#1e3040' } },
            axisLabel: { color: '#4a6a7a', fontFamily: "'Share Tech Mono', monospace", fontSize: 9 },
            splitLine: { show: false },
        },
        yAxis: {
            type: 'value',
            min: cfg.min, max: cfg.max,
            axisLine: { show: false },
            axisLabel: { color: '#4a6a7a', fontFamily: "'Share Tech Mono', monospace", fontSize: 9 },
            splitLine: { lineStyle: { color: '#1a2a38', type: 'dashed' } },
        },
        series: [{
            type: 'line',
            data: [],
            smooth: 0.3,
            symbol: 'none',
            lineStyle: { color: cfg.color, width: 1.5 },
            areaStyle: {
                color: {
                    type: 'linear', x: 0, y: 0, x2: 0, y2: 1,
                    colorStops: [
                        { offset: 0, color: cfg.color + '44' },
                        { offset: 1, color: cfg.color + '00' },
                    ],
                },
            },
            markLine: {
                silent: true, symbol: 'none',
                lineStyle: { type: 'dashed', width: 1 },
                label: { fontFamily: "'Share Tech Mono', monospace", fontSize: 9 },
                data: [
                    { yAxis: cfg.warn, lineStyle: { color: '#ffb300' }, label: { color: '#ffb300', formatter: `WARN ${cfg.warn}` } },
                    { yAxis: cfg.crit, lineStyle: { color: '#ff3d3d' }, label: { color: '#ff3d3d', formatter: `CRIT ${cfg.crit}` } },
                ],
            },
        }],
    });
    trendCharts[param] = chart;
}

// ── Init sparkline ────────────────────────────────────────────────────────────
function initSpark(param) {
    const cfg = PARAMS[param];
    const el = document.getElementById(cfg.sparkId);
    if (!el) return;
    const chart = echarts.init(el, null, { renderer: 'svg' });
    chart.setOption({
        backgroundColor: 'transparent',
        grid: { top: 2, right: 0, bottom: 2, left: 0 },
        xAxis: { type: 'category', show: false, data: [] },
        yAxis: { type: 'value', show: false, min: cfg.min, max: cfg.max },
        series: [{
            type: 'line', data: [], smooth: true, symbol: 'none',
            lineStyle: { color: cfg.color, width: 1.5 },
            areaStyle: { color: cfg.color + '22' },
        }],
    });
    sparkCharts[param] = chart;
}

// ── Apply AGG reading (основной путь — gauge, статус, тренд) ─────────────────
function applyAgg(param, value, timestamp) {
    const cfg = PARAMS[param];
    const st  = state[param];
    st.aggValue = value;

    const status = classify(param, value);
    const ts = new Date(timestamp).toLocaleTimeString('ru-RU', {
        hour: '2-digit', minute: '2-digit', second: '2-digit',
    });

    // Числовое значение
    const valEl = document.getElementById(`val_${param}`);
    if (valEl) valEl.textContent = value.toFixed(2);

    // Статус-пилл
    const pillEl = document.getElementById(`status_${param}`);
    if (pillEl) {
        pillEl.className = `status-pill ${status}`;
        pillEl.textContent = { normal: 'НОРМА', warning: 'ПРЕДУПРЕЖДЕНИЕ', critical: 'КРИТИЧЕСКИЙ' }[status];
    }

    // Карточка (рамка и пульс)
    const cardEl = document.getElementById(`card_${param}`);
    if (cardEl) {
        cardEl.className = `sensor-card${status === 'critical' ? ' state-crit' : status === 'warning' ? ' state-warn' : ''}`;
    }

    // Gauge — стрелка с явным цветом, НЕ 'auto'
    if (gaugeCharts[param]) {
        gaugeCharts[param].setOption({
            series: [{
                data: [{ value }],
                pointer: { itemStyle: { color: pointerColor(param, value) } },
            }],
        });
    }

    // Тренд (строится по agg-значениям — они сглаженные и достоверные)
    st.trendX.push(ts);
    st.trendY.push(value);
    if (st.trendX.length > MAX_HISTORY) { st.trendX.shift(); st.trendY.shift(); }
    if (trendCharts[param]) {
        trendCharts[param].setOption({
            xAxis: { data: st.trendX },
            series: [{ data: st.trendY }],
        });
    }

    // Спарклайн
    st.sparkY.push(value);
    if (st.sparkY.length > MAX_SPARK) st.sparkY.shift();
    if (sparkCharts[param]) {
        sparkCharts[param].setOption({
            xAxis: { data: st.sparkY.map((_, i) => i) },
            series: [{ data: st.sparkY }],
        });
    }

    // Журнал — только при смене статуса
    if (status !== st.lastStatus) {
        addLog(ts, cfg.label, value, cfg.unit, status,
            st.lastStatus ? `было: ${st.lastStatus}` : 'первые данные');
        st.lastStatus = status;
    }
}

// ── Apply RAW reading (только для журнала критических пиков) ─────────────────
function applyRaw(param, value, timestamp) {
    // raw используем только чтобы детектировать мгновенные пики
    // которые могут быть срезаны сглаживанием
    state[param].rawValue = value;
    // Если raw критический, а agg ещё нет — добавляем запись в журнал
    const cfg = PARAMS[param];
    const st  = state[param];
    if (value >= cfg.crit && (st.aggValue === null || st.aggValue < cfg.crit)) {
        const ts = new Date(timestamp).toLocaleTimeString('ru-RU', {
            hour: '2-digit', minute: '2-digit', second: '2-digit',
        });
        addLog(ts, cfg.label + ' [RAW]', value, cfg.unit, 'critical', 'пиковое значение');
    }
}

// ── Log ───────────────────────────────────────────────────────────────────────
let logCount = 0;
function addLog(ts, label, value, unit, status, note) {
    const body = document.getElementById('logBody');
    const empty = body.querySelector('.log-empty');
    if (empty) empty.remove();

    logCount++;
    document.getElementById('logCount').textContent = `${logCount} событий`;

    const statusText = { normal: 'НОРМА', warning: 'ПРЕДУПРЕЖДЕНИЕ', critical: 'КРИТИЧЕСКИЙ' }[status];
    const entry = document.createElement('div');
    entry.className = 'log-entry';
    entry.innerHTML = `
        <span class="log-time">${ts}</span>
        <span class="log-msg">${label}: <b>${value.toFixed(3)} ${unit}</b> — ${statusText}${note ? ' (' + note + ')' : ''}</span>
        <span class="log-badge ${status}">${status.toUpperCase()}</span>
    `;
    body.insertBefore(entry, body.firstChild);
    while (body.children.length > MAX_LOG) body.removeChild(body.lastChild);
}

// ── WebSocket ─────────────────────────────────────────────────────────────────
const wsUrl = `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/ws`;
let ws = null;
let reconnectDelay = 1000;

function setConnState(online) {
    const dot   = document.getElementById('connDot');
    const label = document.getElementById('connLabel');
    dot.className   = `conn-dot ${online ? 'online' : 'offline'}`;
    label.textContent = online ? 'ONLINE' : 'OFFLINE';
    label.style.color = online ? '#00e676' : '#ff3d3d';
    if (online) reconnectDelay = 1000;
}

function connect() {
    ws = new WebSocket(wsUrl);

    ws.onopen  = () => { setConnState(true);  console.log('[ISG] WS connected'); };
    ws.onclose = () => {
        setConnState(false);
        console.log(`[ISG] WS closed, retry in ${reconnectDelay}ms`);
        setTimeout(connect, reconnectDelay);
        reconnectDelay = Math.min(reconnectDelay * 2, 15000);
    };
    ws.onerror = () => ws.close();

    ws.onmessage = evt => {
        let msg;
        try { msg = JSON.parse(evt.data); } catch { return; }

        const { type, parameter: param, value, timestamp } = msg;
        if (!param || value === undefined || !PARAMS[param]) return;

        if (type === 'agg') {
            // Агрегированные данные — источник правды для gauge и статуса
            applyAgg(param, value, timestamp);
        } else if (type === 'raw') {
            // Сырые данные — только для детектирования пиков
            applyRaw(param, value, timestamp);
        }
    };
}

// ── Clock ─────────────────────────────────────────────────────────────────────
function tickClock() {
    document.getElementById('clock').textContent =
        new Date().toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit', second: '2-digit' });
}

// ── Resize ────────────────────────────────────────────────────────────────────
function resizeAll() {
    [...Object.values(gaugeCharts), ...Object.values(trendCharts), ...Object.values(sparkCharts)]
        .forEach(c => c.resize());
}

// ── Boot ──────────────────────────────────────────────────────────────────────
document.addEventListener('DOMContentLoaded', () => {
    Object.keys(PARAMS).forEach(p => { initGauge(p); initTrend(p); initSpark(p); });
    connect();
    tickClock();
    setInterval(tickClock, 1000);
    window.addEventListener('resize', resizeAll);
});