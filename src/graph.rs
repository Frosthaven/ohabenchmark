use anyhow::Result;
use plotters::prelude::*;
use plotters::style::text_anchor::{HPos, Pos, VPos};

use crate::analysis::{BreakReason, StepStatus};
use crate::config::ThresholdConfig;
use crate::output::UrlBenchmarkResults;

/// Error rate line color (red)
const ERROR_COLOR: RGBColor = RGBColor(239, 68, 68);

/// P99 latency line color (blue)
const P99_COLOR: RGBColor = RGBColor(59, 130, 246);

/// Very light grid/outline color
const LIGHT_GRID: RGBColor = RGBColor(230, 230, 230);

/// Error rate threshold lines (percentage, label, color)
const ERROR_THRESHOLDS: &[(f64, &str, RGBColor)] = &[
    (0.1, "0.1% (Payment)", RGBColor(34, 197, 94)), // Green
    (0.5, "0.5% (Core)", RGBColor(132, 204, 22)),   // Lime
    (1.0, "1% (APIs)", RGBColor(234, 179, 8)),      // Yellow
    (2.0, "2% (Non-critical)", RGBColor(239, 68, 68)), // Red (same as ERROR_COLOR)
];

/// Max acceptable p99 latency threshold in milliseconds (3 seconds)
const P99_MAX_ACCEPTABLE_MS: f64 = 3000.0;

/// Business scale categories (min_rate, max_rate, label)
const BUSINESS_SCALES: &[(f64, f64, &str)] = &[
    (1.0, 5.0, "Internal"),
    (5.0, 20.0, "Local"),
    (20.0, 100.0, "Regional"),
    (100.0, 1000.0, "National"),
    (1000.0, f64::MAX, "Global"),
];

/// Color for business scale indicators (matches subtitle)
const SCALE_COLOR: RGBColor = RGBColor(100, 100, 100);

/// DAU estimation constants (users per req/s)
const DAU_PER_RPS_LOW: f64 = 5_000.0;
const DAU_PER_RPS_HIGH: f64 = 15_000.0;

/// Generate a PNG graph showing stacked panels for each URL with error rate and p99 latency
pub fn generate_error_rate_graph(
    url_results: &[UrlBenchmarkResults],
    output_path: &str,
    thresholds: &ThresholdConfig,
) -> Result<()> {
    if url_results.is_empty() {
        return Ok(());
    }

    // Create parent directories if they don't exist
    if let Some(parent) = std::path::Path::new(output_path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let num_urls = url_results.len();
    let width = 2400u32; // 2x size
                         // Dynamic height: 480px per panel + 140px for title/subtitle + 100px for "Requests/Second" + legend
    let panel_height = 480u32; // Increased to fit per-plot labels + business scale labels
    let header_height = 140u32;
    let footer_height = 100u32; // Just "Requests/Second" label + legend
    let height = header_height + (panel_height * num_urls as u32) + footer_height;

    let root = BitMapBackend::new(output_path, (width, height)).into_drawing_area();
    root.fill(&WHITE)?;

    // Calculate shared x-axis range from all data
    let (_, x_max) = calculate_x_range(url_results);
    let x_range = 0.0..(x_max * 1.05);

    // Calculate shared y-axis ranges for normalized comparison across all URLs
    let (error_y_range, p99_y_range) = calculate_shared_y_ranges(url_results, thresholds);

    // Draw main title
    let title_style = TextStyle::from(("sans-serif", 48).into_font())
        .color(&BLACK)
        .pos(Pos::new(HPos::Center, VPos::Top));
    root.draw(&Text::new(
        "Error Rate & P99 Latency vs Requests/Second",
        ((width / 2) as i32, 24),
        title_style,
    ))?;

    // Draw subtitle with business scale ranges
    let subtitle_style = TextStyle::from(("sans-serif", 20).into_font())
        .color(&RGBColor(100, 100, 100))
        .pos(Pos::new(HPos::Center, VPos::Top));
    let subtitle = format_business_scale_subtitle();
    root.draw(&Text::new(
        subtitle,
        ((width / 2) as i32, 80),
        subtitle_style,
    ))?;

    // Draw each URL panel with shared y-axis ranges for comparison
    for (i, url_result) in url_results.iter().enumerate() {
        draw_url_panel(
            &root,
            i,
            num_urls,
            panel_height,
            header_height,
            width,
            url_result,
            &x_range,
            &error_y_range,
            &p99_y_range,
        )?;
    }

    // Draw shared x-axis label at bottom
    let x_label_style = TextStyle::from(("sans-serif", 32).into_font())
        .color(&BLACK)
        .pos(Pos::new(HPos::Center, VPos::Top));
    root.draw(&Text::new(
        "Requests/Second",
        ((width / 2) as i32, (height - 80) as i32),
        x_label_style,
    ))?;

    // Draw legend at the bottom
    draw_legend(&root, width, height)?;

    root.present()?;

    Ok(())
}

/// Draw a single URL panel with dual y-axes
fn draw_url_panel(
    root: &DrawingArea<BitMapBackend, plotters::coord::Shift>,
    panel_index: usize,
    _total_panels: usize,
    panel_height: u32,
    header_height: u32,
    total_width: u32,
    url_result: &UrlBenchmarkResults,
    x_range: &std::ops::Range<f64>,
    error_y_range: &std::ops::Range<f64>,
    p99_y_range: &std::ops::Range<f64>,
) -> Result<()> {
    let y_offset = header_height as i32 + (panel_index as u32 * panel_height) as i32;

    // Panel margins (2x)
    let left_margin = 140i32; // Space for error % y-axis
    let right_margin = 140i32; // Space for p99 y-axis
    let top_margin = 50i32; // Space for URL title
    let bottom_margin = 110i32; // Space for per-plot labels (rate + p99 + error) + business scale labels
    let side_padding = 40i32; // Padding from edge of image

    let chart_width = total_width as i32 - left_margin - right_margin - (side_padding * 2);
    let chart_height = panel_height as i32 - top_margin - bottom_margin;

    // Adjust positions with side padding
    let chart_left = side_padding + left_margin;
    let chart_right = chart_left + chart_width;
    let chart_top = y_offset + top_margin;
    let chart_bottom = y_offset + top_margin + chart_height;

    // Draw URL title
    let url_label = shorten_url(&url_result.url);
    let url_style = TextStyle::from(("sans-serif", 28).into_font())
        .color(&BLACK)
        .pos(Pos::new(HPos::Left, VPos::Top));
    root.draw(&Text::new(
        url_label.clone(),
        (chart_left + 10, y_offset + 10),
        url_style,
    ))?;

    // Draw termination status in red if test ended early
    if let Some(status_text) = format_termination_status(url_result) {
        // Approximate width of URL label (rough estimate: 10px per char for 28pt font)
        let url_width = (url_label.len() as i32) * 14;
        let status_x = chart_left + 10 + url_width + 20; // 20px gap after URL

        let status_style = TextStyle::from(("sans-serif", 28).into_font())
            .color(&ERROR_COLOR)
            .pos(Pos::new(HPos::Left, VPos::Top));
        root.draw(&Text::new(
            status_text,
            (status_x, y_offset + 10),
            status_style,
        ))?;
    }

    // Draw DAU estimate (right-aligned on title line)
    let (dau_estimate, _) = calculate_dau_estimate(url_result);
    let dau_style = TextStyle::from(("sans-serif", 28).into_font())
        .color(&SCALE_COLOR)
        .pos(Pos::new(HPos::Right, VPos::Top));
    root.draw(&Text::new(
        dau_estimate,
        (chart_right - 10, y_offset + 10),
        dau_style,
    ))?;

    // Draw grid lines (very light)
    draw_grid_lines(root, chart_left, chart_right, chart_top, chart_bottom)?;

    // Draw business scale vertical divider lines
    draw_scale_dividers(
        root,
        chart_left,
        chart_right,
        chart_top,
        chart_bottom,
        x_range,
    )?;

    // Draw error rate threshold lines
    draw_threshold_lines(
        root,
        chart_left,
        chart_right,
        chart_top,
        chart_bottom,
        &error_y_range,
    )?;

    // Draw p99 latency threshold line (3 seconds)
    draw_p99_threshold_line(
        root,
        chart_left,
        chart_right,
        chart_top,
        chart_bottom,
        &p99_y_range,
    )?;

    // Collect data points using target_rate for x-axis, excluding the last point if it's a terminal (failure) status
    let mut data: Vec<(f64, f64, f64)> = url_result
        .results
        .iter()
        .map(|r| (r.target_rate as f64, r.error_rate, r.p99_latency_ms))
        .collect();

    // Check if last analysis is a terminal status (failure) - if so, exclude it from the graph
    if let Some(last_analysis) = url_result.analyses.last() {
        let is_terminal = matches!(
            last_analysis.status,
            StepStatus::Break
                | StepStatus::RateLimited
                | StepStatus::Blocked
                | StepStatus::Hung
                | StepStatus::Gone
        );
        if is_terminal && data.len() > 1 {
            data.pop();
        }
    }

    if data.is_empty() {
        return Ok(());
    }

    // Draw error rate line and points (left y-axis, red) - only visible near non-zero points
    draw_error_data_line(
        root,
        &data
            .iter()
            .map(|(x, err, _)| (*x, *err))
            .collect::<Vec<_>>(),
        x_range,
        error_y_range,
        chart_left,
        chart_right,
        chart_top,
        chart_bottom,
    )?;

    // Draw p99 latency line and points (right y-axis, blue/purple based on threshold)
    draw_p99_data_line(
        root,
        &data
            .iter()
            .map(|(x, _, p99)| (*x, *p99))
            .collect::<Vec<_>>(),
        x_range,
        p99_y_range,
        chart_left,
        chart_right,
        chart_top,
        chart_bottom,
    )?;

    // Draw left y-axis (p99 latency ms)
    draw_y_axis_left(
        root,
        chart_left,
        chart_top,
        chart_bottom,
        &p99_y_range,
        P99_COLOR,
        side_padding,
    )?;

    // Draw right y-axis (error rate %)
    draw_y_axis_right(
        root,
        chart_right,
        chart_top,
        chart_bottom,
        &error_y_range,
        ERROR_COLOR,
        total_width as i32 - side_padding,
    )?;

    // Draw per-plot-point x-axis labels (using this URL's target rates)
    let plot_labels_y = chart_bottom + 15;
    draw_plot_point_labels(root, chart_left, chart_right, plot_labels_y, x_range, &data)?;

    // Draw business scale labels below the plot point labels (rate + p99 + error rate)
    let business_scale_y = chart_bottom + 60;
    draw_business_scales(root, chart_left, chart_right, business_scale_y, x_range)?;

    Ok(())
}

/// Draw grid lines (horizontal only - vertical lines are drawn by scale dividers)
fn draw_grid_lines(
    root: &DrawingArea<BitMapBackend, plotters::coord::Shift>,
    left: i32,
    right: i32,
    top: i32,
    bottom: i32,
) -> Result<()> {
    let grid_style = ShapeStyle {
        color: LIGHT_GRID.to_rgba(),
        filled: false,
        stroke_width: 1,
    };

    // Horizontal grid lines only (5 lines)
    for i in 0..=4 {
        let y = top + (bottom - top) * i / 4;
        root.draw(&PathElement::new(
            vec![(left, y), (right, y)],
            grid_style.clone(),
        ))?;
    }

    Ok(())
}

/// Draw solid vertical divider lines at business scale boundaries
fn draw_scale_dividers(
    root: &DrawingArea<BitMapBackend, plotters::coord::Shift>,
    left: i32,
    right: i32,
    top: i32,
    bottom: i32,
    x_range: &std::ops::Range<f64>,
) -> Result<()> {
    let chart_width = (right - left) as f64;
    let x_size = x_range.end - x_range.start;

    // Collect all boundary points from business scales
    let mut boundaries: Vec<f64> = Vec::new();
    for &(min_rate, max_rate, _) in BUSINESS_SCALES {
        if min_rate > x_range.start && min_rate < x_range.end {
            boundaries.push(min_rate);
        }
        if max_rate != f64::MAX && max_rate > x_range.start && max_rate < x_range.end {
            if !boundaries.contains(&max_rate) {
                boundaries.push(max_rate);
            }
        }
    }

    // Draw solid vertical lines at each boundary
    let line_style = ShapeStyle {
        color: SCALE_COLOR.mix(0.5).to_rgba(),
        filled: false,
        stroke_width: 1,
    };

    for boundary in boundaries {
        let x = left + (((boundary - x_range.start) / x_size) * chart_width) as i32;

        // Draw solid vertical line
        root.draw(&PathElement::new(
            vec![(x, top), (x, bottom)],
            line_style.clone(),
        ))?;
    }

    Ok(())
}

/// Draw error rate threshold dashed lines
fn draw_threshold_lines(
    root: &DrawingArea<BitMapBackend, plotters::coord::Shift>,
    left: i32,
    right: i32,
    top: i32,
    bottom: i32,
    error_range: &std::ops::Range<f64>,
) -> Result<()> {
    let chart_height = (bottom - top) as f64;
    let range_size = error_range.end - error_range.start;

    for &(threshold, _label, color) in ERROR_THRESHOLDS {
        // Only draw if threshold is within visible range
        if threshold >= error_range.start && threshold < error_range.end {
            // Convert threshold to y pixel position
            let y_ratio = (threshold - error_range.start) / range_size;
            let y = bottom - (y_ratio * chart_height) as i32;

            // Draw dashed line
            let dash_style = ShapeStyle {
                color: color.mix(0.6).to_rgba(),
                filled: false,
                stroke_width: 2,
            };

            let dash_len = 16i32;
            let gap_len = 8i32;
            let mut x = left;
            while x < right {
                let x_end = (x + dash_len).min(right);
                root.draw(&PathElement::new(
                    vec![(x, y), (x_end, y)],
                    dash_style.clone(),
                ))?;
                x += dash_len + gap_len;
            }
        }
    }

    Ok(())
}

/// Draw p99 latency max acceptable threshold line (dotted, blue)
fn draw_p99_threshold_line(
    root: &DrawingArea<BitMapBackend, plotters::coord::Shift>,
    left: i32,
    right: i32,
    top: i32,
    bottom: i32,
    p99_range: &std::ops::Range<f64>,
) -> Result<()> {
    let chart_height = (bottom - top) as f64;
    let range_size = p99_range.end - p99_range.start;

    // Only draw if threshold is within visible range
    if P99_MAX_ACCEPTABLE_MS >= p99_range.start && P99_MAX_ACCEPTABLE_MS < p99_range.end {
        // Convert threshold to y pixel position
        let y_ratio = (P99_MAX_ACCEPTABLE_MS - p99_range.start) / range_size;
        let y = bottom - (y_ratio * chart_height) as i32;

        // Draw dotted line (shorter dashes than error thresholds)
        let dash_style = ShapeStyle {
            color: P99_COLOR.mix(0.6).to_rgba(),
            filled: false,
            stroke_width: 2,
        };

        let dash_len = 8i32;
        let gap_len = 8i32;
        let mut x = left;
        while x < right {
            let x_end = (x + dash_len).min(right);
            root.draw(&PathElement::new(
                vec![(x, y), (x_end, y)],
                dash_style.clone(),
            ))?;
            x += dash_len + gap_len;
        }
    }

    Ok(())
}

/// Draw p99 latency line with threshold-based coloring
/// Points and segments turn purple when above the 3s threshold
fn draw_p99_data_line(
    root: &DrawingArea<BitMapBackend, plotters::coord::Shift>,
    data: &[(f64, f64)], // (x, p99_ms)
    x_range: &std::ops::Range<f64>,
    y_range: &std::ops::Range<f64>,
    left: i32,
    right: i32,
    top: i32,
    bottom: i32,
) -> Result<()> {
    if data.is_empty() {
        return Ok(());
    }

    let chart_width = (right - left) as f64;
    let chart_height = (bottom - top) as f64;
    let x_size = x_range.end - x_range.start;
    let y_size = y_range.end - y_range.start;

    // Convert data points to pixel coordinates, keeping track of original values
    let points: Vec<((i32, i32), f64)> = data
        .iter()
        .map(|(x, y)| {
            let px = left + (((*x - x_range.start) / x_size) * chart_width) as i32;
            let py = bottom - (((*y - y_range.start) / y_size) * chart_height) as i32;
            ((px, py), *y)
        })
        .collect();

    // Draw translucent area fill below the line (use blue for the fill)
    if points.len() >= 2 {
        let fill_color = P99_COLOR.mix(0.15);
        let mut area_points: Vec<(i32, i32)> = Vec::new();

        area_points.push((points[0].0 .0, bottom));
        for &(pt, _) in &points {
            area_points.push(pt);
        }
        area_points.push((points[points.len() - 1].0 .0, bottom));
        area_points.push((points[0].0 .0, bottom));

        root.draw(&Polygon::new(area_points, fill_color.filled()))?;
    }

    // Draw line segments (always blue)
    let line_style = ShapeStyle {
        color: P99_COLOR.to_rgba(),
        filled: false,
        stroke_width: 4,
    };

    for i in 0..points.len().saturating_sub(1) {
        let (pt1, _) = points[i];
        let (pt2, _) = points[i + 1];
        root.draw(&PathElement::new(vec![pt1, pt2], line_style.clone()))?;
    }

    // Draw points (always blue)
    for &((px, py), _) in &points {
        root.draw(&Circle::new((px, py), 4, P99_COLOR.filled()))?;
    }

    Ok(())
}

/// Draw error rate line, only showing segments adjacent to non-zero points
fn draw_error_data_line(
    root: &DrawingArea<BitMapBackend, plotters::coord::Shift>,
    data: &[(f64, f64)], // (x, error_rate)
    x_range: &std::ops::Range<f64>,
    y_range: &std::ops::Range<f64>,
    left: i32,
    right: i32,
    top: i32,
    bottom: i32,
) -> Result<()> {
    if data.is_empty() {
        return Ok(());
    }

    let chart_width = (right - left) as f64;
    let chart_height = (bottom - top) as f64;
    let x_size = x_range.end - x_range.start;
    let y_size = y_range.end - y_range.start;

    // Convert data points to pixel coordinates, keeping track of original values
    let points: Vec<((i32, i32), f64)> = data
        .iter()
        .map(|(x, y)| {
            let px = left + (((*x - x_range.start) / x_size) * chart_width) as i32;
            let py = bottom - (((*y - y_range.start) / y_size) * chart_height) as i32;
            ((px, py), *y)
        })
        .collect();

    // Determine which points are "visible" (non-zero or adjacent to non-zero)
    let mut visible: Vec<bool> = vec![false; points.len()];
    for i in 0..points.len() {
        if points[i].1 > 0.0 {
            visible[i] = true;
            // Also mark adjacent points as visible
            if i > 0 {
                visible[i - 1] = true;
            }
            if i + 1 < points.len() {
                visible[i + 1] = true;
            }
        }
    }

    // Draw translucent area fill only for visible segments
    if points.len() >= 2 {
        let fill_color = ERROR_COLOR.mix(0.15);

        // Find contiguous visible segments and draw area for each
        let mut i = 0;
        while i < points.len() {
            if visible[i] {
                // Start of a visible segment
                let start = i;
                while i < points.len() && visible[i] {
                    i += 1;
                }
                let end = i;

                if end - start >= 2 {
                    let mut area_points: Vec<(i32, i32)> = Vec::new();
                    area_points.push((points[start].0 .0, bottom));
                    for j in start..end {
                        area_points.push(points[j].0);
                    }
                    area_points.push((points[end - 1].0 .0, bottom));
                    area_points.push((points[start].0 .0, bottom));

                    root.draw(&Polygon::new(area_points, fill_color.filled()))?;
                }
            } else {
                i += 1;
            }
        }
    }

    // Draw line segments only where both endpoints are visible
    let line_style = ShapeStyle {
        color: ERROR_COLOR.to_rgba(),
        filled: false,
        stroke_width: 4,
    };

    for i in 0..points.len().saturating_sub(1) {
        if visible[i] && visible[i + 1] {
            root.draw(&PathElement::new(
                vec![points[i].0, points[i + 1].0],
                line_style.clone(),
            ))?;
        }
    }

    // Draw points only for visible points
    for i in 0..points.len() {
        if visible[i] {
            let (px, py) = points[i].0;
            root.draw(&Circle::new((px, py), 4, ERROR_COLOR.filled()))?;
        }
    }

    Ok(())
}

/// Draw left y-axis with labels (p99 latency)
fn draw_y_axis_left(
    root: &DrawingArea<BitMapBackend, plotters::coord::Shift>,
    chart_left: i32,
    top: i32,
    bottom: i32,
    range: &std::ops::Range<f64>,
    color: RGBColor,
    side_padding: i32,
) -> Result<()> {
    let label_style = TextStyle::from(("sans-serif", 22).into_font())
        .color(&color)
        .pos(Pos::new(HPos::Right, VPos::Center));

    let chart_height = (bottom - top) as f64;
    let range_size = range.end - range.start;

    // Draw 5 tick labels
    for i in 0..=4 {
        let ratio = i as f64 / 4.0;
        let value = range.start + ratio * range_size;
        let y = bottom - (ratio * chart_height) as i32;

        let label = format_latency_short(value);

        root.draw(&Text::new(label, (chart_left - 10, y), label_style.clone()))?;
    }

    // Draw axis label - "p99"
    let axis_label_style = TextStyle::from(("sans-serif", 24).into_font())
        .color(&color)
        .pos(Pos::new(HPos::Center, VPos::Center));

    let mid_y = (top + bottom) / 2;
    root.draw(&Text::new(
        "p99",
        (side_padding + 10, mid_y),
        axis_label_style,
    ))?;

    Ok(())
}

/// Draw right y-axis with labels (error rate %)
fn draw_y_axis_right(
    root: &DrawingArea<BitMapBackend, plotters::coord::Shift>,
    chart_right: i32,
    top: i32,
    bottom: i32,
    range: &std::ops::Range<f64>,
    color: RGBColor,
    right_edge: i32,
) -> Result<()> {
    let label_style = TextStyle::from(("sans-serif", 22).into_font())
        .color(&color)
        .pos(Pos::new(HPos::Left, VPos::Center));

    let chart_height = (bottom - top) as f64;
    let range_size = range.end - range.start;

    // Draw 5 tick labels
    for i in 0..=4 {
        let ratio = i as f64 / 4.0;
        let value = range.start + ratio * range_size;
        let y = bottom - (ratio * chart_height) as i32;

        let label = if value < 10.0 {
            format!("{:.1}%", value)
        } else {
            format!("{:.0}%", value)
        };
        root.draw(&Text::new(
            label,
            (chart_right + 10, y),
            label_style.clone(),
        ))?;
    }

    // Draw axis label - "Error %"
    let axis_label_style = TextStyle::from(("sans-serif", 24).into_font())
        .color(&color)
        .pos(Pos::new(HPos::Center, VPos::Center));

    let mid_y = (top + bottom) / 2;
    root.draw(&Text::new(
        "Error %",
        (right_edge - 30, mid_y),
        axis_label_style,
    ))?;

    Ok(())
}

/// Format latency value for axis labels
fn format_latency_short(ms: f64) -> String {
    if ms < 1.0 {
        format!("{:.0}us", ms * 1000.0)
    } else if ms < 1000.0 {
        format!("{:.0}ms", ms)
    } else {
        format!("{:.1}s", ms / 1000.0)
    }
}

/// Draw per-plot-point req/s labels below the chart with p99 and error rate values
fn draw_plot_point_labels(
    root: &DrawingArea<BitMapBackend, plotters::coord::Shift>,
    chart_left: i32,
    chart_right: i32,
    label_y: i32,
    x_range: &std::ops::Range<f64>,
    data: &[(f64, f64, f64)], // (target_rate, error_rate, p99_latency_ms)
) -> Result<()> {
    let chart_width = (chart_right - chart_left) as f64;
    let x_size = x_range.end - x_range.start;

    // Rate label style (black)
    let rate_label_style = TextStyle::from(("sans-serif", 16).into_font())
        .color(&BLACK)
        .pos(Pos::new(HPos::Center, VPos::Top));

    // P99 label style (blue)
    let p99_label_style = TextStyle::from(("sans-serif", 14).into_font())
        .color(&P99_COLOR)
        .pos(Pos::new(HPos::Center, VPos::Top));

    // Error rate label style (red)
    let error_label_style = TextStyle::from(("sans-serif", 14).into_font())
        .color(&ERROR_COLOR)
        .pos(Pos::new(HPos::Center, VPos::Top));

    for &(rate, error_rate, p99_ms) in data {
        if rate < x_range.start || rate > x_range.end {
            continue;
        }

        // Calculate x position
        let x = chart_left + (((rate - x_range.start) / x_size) * chart_width) as i32;

        // Format the rate label - always whole numbers
        let rate_label = if rate >= 1000.0 {
            format!("{:.0}k", rate / 1000.0)
        } else {
            format!("{:.0}", rate)
        };

        // Draw rate label
        root.draw(&Text::new(
            rate_label,
            (x, label_y),
            rate_label_style.clone(),
        ))?;

        // Draw p99 value below rate (always shown)
        let p99_label = format_latency_short(p99_ms);
        root.draw(&Text::new(
            p99_label,
            (x, label_y + 16),
            p99_label_style.clone(),
        ))?;

        // Draw error rate below p99 (only if > 0)
        if error_rate > 0.0 {
            let error_label = if error_rate < 10.0 {
                format!("{:.1}%", error_rate)
            } else {
                format!("{:.0}%", error_rate)
            };
            root.draw(&Text::new(
                error_label,
                (x, label_y + 30),
                error_label_style.clone(),
            ))?;
        }
    }

    Ok(())
}

/// Draw business scale indicators below the chart
fn draw_business_scales(
    root: &DrawingArea<BitMapBackend, plotters::coord::Shift>,
    chart_left: i32,
    chart_right: i32,
    label_y: i32,
    x_range: &std::ops::Range<f64>,
) -> Result<()> {
    let chart_width = (chart_right - chart_left) as f64;
    let x_size = x_range.end - x_range.start;

    let label_style = TextStyle::from(("sans-serif", 18).into_font())
        .color(&SCALE_COLOR)
        .pos(Pos::new(HPos::Center, VPos::Top));

    // Draw labels at the boundary lines between scales
    // Each boundary is the max_rate of one scale (and min_rate of the next)
    for &(_, max_rate, label) in BUSINESS_SCALES {
        // Skip the last scale (Global) which has no upper boundary
        if max_rate == f64::MAX {
            continue;
        }

        // Skip if boundary is outside visible range
        if max_rate <= x_range.start || max_rate >= x_range.end {
            continue;
        }

        // Convert boundary to pixel position
        let label_x = chart_left + (((max_rate - x_range.start) / x_size) * chart_width) as i32;

        // Draw label centered on the boundary line
        root.draw(&Text::new(label, (label_x, label_y), label_style.clone()))?;
    }

    Ok(())
}

/// Draw legend at the bottom of the chart
fn draw_legend(
    root: &DrawingArea<BitMapBackend, plotters::coord::Shift>,
    width: u32,
    height: u32,
) -> Result<()> {
    let legend_y = (height - 40) as i32;
    let left_x = 60i32;

    // Left side: P99 latency legend items
    let p99_line_start = left_x;
    root.draw(&PathElement::new(
        vec![(p99_line_start, legend_y), (p99_line_start + 40, legend_y)],
        P99_COLOR.stroke_width(4),
    ))?;

    let label_style = TextStyle::from(("sans-serif", 22).into_font())
        .color(&P99_COLOR)
        .pos(Pos::new(HPos::Left, VPos::Center));
    root.draw(&Text::new(
        "p99 Latency",
        (p99_line_start + 50, legend_y),
        label_style,
    ))?;

    // P99 threshold legend item (dotted line + "3s max")
    let p99_threshold_start = left_x + 200;
    let dash_style = ShapeStyle {
        color: P99_COLOR.mix(0.6).to_rgba(),
        filled: false,
        stroke_width: 4,
    };
    // Draw dotted line (3 dots)
    root.draw(&PathElement::new(
        vec![
            (p99_threshold_start, legend_y),
            (p99_threshold_start + 8, legend_y),
        ],
        dash_style.clone(),
    ))?;
    root.draw(&PathElement::new(
        vec![
            (p99_threshold_start + 16, legend_y),
            (p99_threshold_start + 24, legend_y),
        ],
        dash_style.clone(),
    ))?;
    root.draw(&PathElement::new(
        vec![
            (p99_threshold_start + 32, legend_y),
            (p99_threshold_start + 40, legend_y),
        ],
        dash_style,
    ))?;

    let threshold_text_style = TextStyle::from(("sans-serif", 20).into_font())
        .color(&P99_COLOR)
        .pos(Pos::new(HPos::Left, VPos::Center));
    root.draw(&Text::new(
        "3s Max Acceptable",
        (p99_threshold_start + 50, legend_y),
        threshold_text_style,
    ))?;

    // Right side: Error Rate (solid) + Threshold lines with their descriptions
    // Layout: [--- Error Rate] [--- 0.1% Payment] [--- 0.5% Core] [--- 1% APIs] [--- 2% Non-critical]
    let right_x = (width - 60) as i32;
    let mut x_pos = right_x;

    // Draw thresholds right-to-left so they end at right edge
    // Reverse order: 2%, 1%, 0.5%, 0.1%
    let thresholds_reversed: Vec<_> = ERROR_THRESHOLDS.iter().rev().collect();

    for &(threshold, label_text, color) in &thresholds_reversed {
        // Format: "X% Label"
        let text = format!(
            "{}% {}",
            threshold,
            label_text
                .split('(')
                .nth(1)
                .unwrap_or(label_text)
                .trim_end_matches(')')
        );
        let text_width = (text.len() as i32) * 12; // Approximate width (2x)
        let line_width = 30i32;
        let spacing = 30i32;

        // Draw label with threshold color
        let threshold_label_style = TextStyle::from(("sans-serif", 20).into_font())
            .color(&color)
            .pos(Pos::new(HPos::Left, VPos::Center));

        let label_x = x_pos - text_width;
        root.draw(&Text::new(text, (label_x, legend_y), threshold_label_style))?;

        // Draw dashed line segment before label
        let line_end = label_x - 10;
        let line_start = line_end - line_width;

        // Draw dashed line (3 small dashes)
        let dash_style = ShapeStyle {
            color: color.to_rgba(),
            filled: false,
            stroke_width: 4,
        };
        root.draw(&PathElement::new(
            vec![(line_start, legend_y), (line_start + 8, legend_y)],
            dash_style.clone(),
        ))?;
        root.draw(&PathElement::new(
            vec![(line_start + 12, legend_y), (line_start + 20, legend_y)],
            dash_style.clone(),
        ))?;
        root.draw(&PathElement::new(
            vec![(line_start + 24, legend_y), (line_end, legend_y)],
            dash_style,
        ))?;

        x_pos = line_start - spacing;
    }

    // Add Error Rate (solid red line) as the leftmost item in the error rates group
    let error_text = "Error Rate";
    let error_text_width = (error_text.len() as i32) * 12;
    let error_line_width = 30i32;

    let error_label_style = TextStyle::from(("sans-serif", 20).into_font())
        .color(&ERROR_COLOR)
        .pos(Pos::new(HPos::Left, VPos::Center));

    let error_label_x = x_pos - error_text_width;
    root.draw(&Text::new(
        error_text,
        (error_label_x, legend_y),
        error_label_style,
    ))?;

    // Draw solid line segment before label
    let error_line_end = error_label_x - 10;
    let error_line_start = error_line_end - error_line_width;

    root.draw(&PathElement::new(
        vec![(error_line_start, legend_y), (error_line_end, legend_y)],
        ERROR_COLOR.stroke_width(4),
    ))?;

    Ok(())
}

/// Calculate the x-axis range (req/s) from all results
/// Uses target rate data range with small padding for better visualization
fn calculate_x_range(url_results: &[UrlBenchmarkResults]) -> (f64, f64) {
    let mut min_rate = f64::MAX;
    let mut max_rate = 0f64;

    for url_result in url_results {
        for result in &url_result.results {
            let rate = result.target_rate as f64;
            if rate > 0.0 {
                min_rate = min_rate.min(rate);
                max_rate = max_rate.max(rate);
            }
        }
    }

    if min_rate == f64::MAX {
        min_rate = 0.0;
    }

    // Just use the target max with a small padding - no need to extend to full scale boundaries
    // The 1.05 multiplier in the main function adds 5% padding

    // Return both but we use 0 for start
    (min_rate, max_rate)
}

/// Calculate y-axis ranges for a single URL result
fn calculate_y_ranges(url_result: &UrlBenchmarkResults) -> (f64, f64) {
    let mut max_error_rate = 0f64;
    let mut max_p99 = 0f64;

    for result in &url_result.results {
        max_error_rate = max_error_rate.max(result.error_rate);
        max_p99 = max_p99.max(result.p99_latency_ms);
    }

    (max_error_rate, max_p99)
}

/// Calculate shared y-axis ranges across all URL results for normalized comparison
fn calculate_shared_y_ranges(
    url_results: &[UrlBenchmarkResults],
    thresholds: &ThresholdConfig,
) -> (std::ops::Range<f64>, std::ops::Range<f64>) {
    let mut max_error_rate = 0f64;
    let mut max_p99 = 0f64;

    for url_result in url_results {
        let (err, p99) = calculate_y_ranges(url_result);
        max_error_rate = max_error_rate.max(err);
        max_p99 = max_p99.max(p99);
    }

    // Fixed y-axis ranges: error rate 0-50% (or threshold if smaller), p99 0-4s (or threshold if smaller)
    let error_cap = 50.0_f64.min(thresholds.max_error_rate);
    let p99_cap = 4000.0_f64.min(thresholds.max_p99_ms as f64);

    // Ensure p99 cap is at least 10% above the threshold line so it's always visible
    let p99_cap = p99_cap.max(P99_MAX_ACCEPTABLE_MS * 1.1);

    let error_y_range = 0f64..error_cap;
    let p99_y_range = 0f64..p99_cap;

    (error_y_range, p99_y_range)
}

/// Shorten a URL for display
fn shorten_url(url: &str) -> String {
    let url = url
        .trim_start_matches("https://")
        .trim_start_matches("http://");

    // Remove trailing slash
    let url = url.trim_end_matches('/');

    // If URL is still too long, truncate with ellipsis
    if url.len() > 60 {
        format!("{}...", &url[..57])
    } else {
        url.to_string()
    }
}

/// Format a rate value with shorthand (k for thousands, etc.)
fn format_rate_short(rate: f64) -> String {
    if rate >= 1000.0 {
        let k = rate / 1000.0;
        if k >= 100.0 {
            format!("{:.0}k", k)
        } else if k >= 10.0 {
            let s = format!("{:.1}", k);
            format!("{}k", s.trim_end_matches('0').trim_end_matches('.'))
        } else {
            let s = format!("{:.1}", k);
            format!("{}k", s.trim_end_matches('0').trim_end_matches('.'))
        }
    } else if rate >= 100.0 {
        format!("{:.0}", rate)
    } else if rate >= 10.0 {
        let s = format!("{:.1}", rate);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        let s = format!("{:.1}", rate);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// Format termination status for display on the chart
/// Returns None if the test completed normally (no early termination)
fn format_termination_status(url_result: &UrlBenchmarkResults) -> Option<String> {
    let last_analysis = url_result.analyses.last()?;

    match last_analysis.status {
        StepStatus::Break => {
            let reason = match &last_analysis.break_reason {
                BreakReason::ErrorRate(rate) => format!("Error Rate ({:.1}%)", rate),
                BreakReason::P99Latency(ms) => format!("P99 Latency ({:.0}ms)", ms),
                BreakReason::ThroughputDegradation(pct) => {
                    format!("Throughput Degradation ({:.0}%)", pct)
                }
                BreakReason::Hung => "Server Hung".to_string(),
                BreakReason::NoResponses => "No Responses".to_string(),
                _ => "Threshold Exceeded".to_string(),
            };
            Some(format!("BREAK: {}", reason))
        }
        StepStatus::RateLimited => Some("RATE LIMITED".to_string()),
        StepStatus::Blocked => Some("BLOCKED".to_string()),
        StepStatus::Hung => Some("CONNECTION HUNG".to_string()),
        StepStatus::Gone => Some("NO RESPONSE".to_string()),
        StepStatus::Ok | StepStatus::Warning => None,
    }
}

/// Format a number in shorthand (5k, 1.2M, etc.)
fn format_short_number(n: f64) -> String {
    if n >= 1_000_000.0 {
        format!("{:.1}M", n / 1_000_000.0)
    } else if n >= 1_000.0 {
        format!("{:.0}k", n / 1_000.0)
    } else {
        format!("{:.0}", n)
    }
}

/// Calculate estimated DAU based on max successful rate below p99 threshold
/// Returns (formatted_string, did_break)
/// - did_break: true if test terminated early, false if completed all steps
fn calculate_dau_estimate(url_result: &UrlBenchmarkResults) -> (String, bool) {
    // Check if test ended with a break/failure status
    let did_break = url_result.analyses.last().map_or(false, |a| {
        !matches!(a.status, StepStatus::Ok | StepStatus::Warning)
    });

    // Find max rate where status is Ok/Warning AND p99 < threshold
    let max_qualifying_rate: Option<u32> = url_result
        .results
        .iter()
        .zip(url_result.analyses.iter())
        .filter(|(_, a)| matches!(a.status, StepStatus::Ok | StepStatus::Warning))
        .filter(|(r, _)| r.p99_latency_ms < P99_MAX_ACCEPTABLE_MS)
        .map(|(r, _)| r.target_rate)
        .max();

    match max_qualifying_rate {
        Some(rate) => {
            let dau_low = rate as f64 * DAU_PER_RPS_LOW;
            let dau_high = rate as f64 * DAU_PER_RPS_HIGH;

            let estimate = if did_break {
                format!(
                    "~{} - {} DAU",
                    format_short_number(dau_low),
                    format_short_number(dau_high)
                )
            } else {
                // Test completed without breaking - show as minimum
                format!("~{}+ DAU", format_short_number(dau_low))
            };
            (estimate, did_break)
        }
        None => {
            // No qualifying rate - endpoint couldn't handle even minimal load
            ("<5k DAU".to_string(), did_break)
        }
    }
}

/// Generate the subtitle with business scale ranges
fn format_business_scale_subtitle() -> String {
    let parts: Vec<String> = BUSINESS_SCALES
        .iter()
        .map(|&(min, max, name)| {
            let max_str = if max == f64::MAX {
                "+".to_string()
            } else {
                format!("-{}", format_rate_short(max))
            };
            format!("{} {}{}", name, format_rate_short(min), max_str)
        })
        .collect();

    format!("Classed by {}", parts.join(" | "))
}
