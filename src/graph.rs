use anyhow::Result;
use plotters::prelude::*;
use plotters::style::text_anchor::{HPos, Pos, VPos};

use crate::output::UrlBenchmarkResults;

/// Color palette for different URLs (visually distinct colors)
const COLORS: &[RGBColor] = &[
    RGBColor(59, 130, 246), // Blue
    RGBColor(239, 68, 68),  // Red
    RGBColor(34, 197, 94),  // Green
    RGBColor(168, 85, 247), // Purple
    RGBColor(249, 115, 22), // Orange
    RGBColor(236, 72, 153), // Pink
    RGBColor(20, 184, 166), // Teal
    RGBColor(234, 179, 8),  // Yellow
];

/// Threshold lines for error rates (ordered from strictest to most lenient for display)
const THRESHOLDS: &[(f64, &str, RGBColor)] = &[
    (2.0, "Non-critical", RGBColor(249, 115, 22)), // Orange
    (1.0, "APIs", RGBColor(234, 179, 8)),          // Yellow
    (0.5, "Core App", RGBColor(34, 197, 94)),      // Green
    (0.1, "Payment", RGBColor(34, 197, 94)),       // Green
];

/// Generate a PNG graph showing error rate vs requests/second for all URLs
pub fn generate_error_rate_graph(
    url_results: &[UrlBenchmarkResults],
    output_path: &str,
) -> Result<()> {
    // Create parent directories if they don't exist
    if let Some(parent) = std::path::Path::new(output_path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let width = 1200u32;
    let height = 700u32;

    let root = BitMapBackend::new(output_path, (width, height)).into_drawing_area();
    root.fill(&WHITE)?;

    // Calculate axis ranges from all data
    let (_min_rate, max_rate, max_error_rate) = calculate_ranges(url_results);

    // Add some padding to the ranges
    let x_range = 0f64..(max_rate * 1.05);
    let y_range = 0f64..(max_error_rate * 1.1).max(5.0); // At least show 0-5% range

    let mut chart = ChartBuilder::on(&root)
        .caption(
            "Error Rate vs Requests/Second",
            ("sans-serif", 28).into_font(),
        )
        .margin(20)
        .x_label_area_size(50)
        .y_label_area_size(60)
        .build_cartesian_2d(x_range.clone(), y_range.clone())?;

    chart
        .configure_mesh()
        .x_desc("Requests/Second")
        .y_desc("Error Rate (%)")
        .x_label_style(("sans-serif", 16))
        .y_label_style(("sans-serif", 16))
        .axis_desc_style(("sans-serif", 18))
        .light_line_style(RGBColor(230, 230, 230))
        .draw()?;

    // Draw threshold lines (without labels - labels are in the legend box)
    let x_max = max_rate * 1.05;
    let y_max = (max_error_rate * 1.1).max(5.0);
    for &(threshold, _label, color) in THRESHOLDS {
        // Only draw if threshold is within visible range
        if threshold < y_max {
            // Draw dashed horizontal line (thin)
            let dashed_style = ShapeStyle {
                color: color.mix(0.7).to_rgba(),
                filled: false,
                stroke_width: 1,
            };

            // Create dashed line by drawing segments
            let dash_len = x_max / 80.0;
            let gap_len = x_max / 160.0;
            let mut x = 0.0;
            while x < x_max {
                let x_end = (x + dash_len).min(x_max);
                chart.draw_series(LineSeries::new(
                    vec![(x, threshold), (x_end, threshold)],
                    dashed_style.clone(),
                ))?;
                x += dash_len + gap_len;
            }
        }
    }

    // Plot each URL's data
    for (i, url_result) in url_results.iter().enumerate() {
        let color = COLORS[i % COLORS.len()];
        let label = shorten_url(&url_result.url);

        // Collect data points: (actual_rate, error_rate)
        let data: Vec<(f64, f64)> = url_result
            .results
            .iter()
            .map(|r| (r.actual_rate, r.error_rate))
            .collect();

        if data.is_empty() {
            continue;
        }

        // Draw translucent filled area under the line
        let fill_color = color.mix(0.15);
        chart.draw_series(AreaSeries::new(data.clone(), 0.0, fill_color))?;

        // Draw the line
        chart
            .draw_series(LineSeries::new(data.clone(), color.stroke_width(2)))?
            .label(&label)
            .legend(move |(x, y)| {
                PathElement::new(vec![(x, y), (x + 20, y)], color.stroke_width(2))
            });

        // Draw smaller points on the line
        chart.draw_series(
            data.iter()
                .map(|(x, y)| Circle::new((*x, *y), 3, color.filled())),
        )?;
    }

    // Draw legend
    chart
        .configure_series_labels()
        .background_style(WHITE.mix(0.9))
        .border_style(BLACK.stroke_width(1))
        .label_font(("sans-serif", 14))
        .position(SeriesLabelPosition::UpperLeft)
        .margin(10)
        .draw()?;

    // Draw threshold legend box below the URL legend
    // Estimate URL legend height: ~22px per entry + padding
    let url_count = url_results.len();
    let url_legend_height_px = 20 + (url_count as i32 * 22);

    // Convert pixel offset to data coordinates (approximate)
    // The chart area is roughly height - margins, y_max maps to top
    let chart_height_px = height as f64 - 70.0 - 50.0; // minus top margin/caption and bottom label area
    let chart_width_px = width as f64 - 60.0 - 20.0; // minus y_label_area and right margin
    let px_per_unit_y = chart_height_px / y_max;
    let px_per_unit_x = chart_width_px / x_max;
    let url_legend_offset = (url_legend_height_px as f64 + 20.0) / px_per_unit_y;

    // Position threshold legend below URL legend
    // URL legend is at margin=10 from chart edge, so ~10px from left of chart area
    let legend_y_start = y_max - url_legend_offset;
    let line_height = y_max * 0.045;
    let line_width = x_max * 0.025;
    // Align with URL legend: 10px margin from chart area left edge
    let legend_x = 10.0 / px_per_unit_x;

    // Draw background box
    // URL legend with entries like "sbltn.com", "eidexgroup.com" is ~150px wide
    // Threshold entries like "< 2% Non-critical" are similar length
    let box_width = 150.0 / px_per_unit_x;
    let box_height = line_height * 5.8;
    let box_x = legend_x - 5.0 / px_per_unit_x; // 5px padding left of legend_x
    let box_y = legend_y_start + line_height * 0.3;

    chart.draw_series(std::iter::once(Rectangle::new(
        [(box_x, box_y), (box_x + box_width, box_y - box_height)],
        ShapeStyle {
            color: WHITE.mix(0.9).to_rgba(),
            filled: true,
            stroke_width: 0,
        },
    )))?;
    chart.draw_series(std::iter::once(Rectangle::new(
        [(box_x, box_y), (box_x + box_width, box_y - box_height)],
        ShapeStyle {
            color: BLACK.to_rgba(),
            filled: false,
            stroke_width: 1,
        },
    )))?;

    // Title
    let title_style = TextStyle::from(("sans-serif", 12).into_font())
        .color(&BLACK)
        .pos(Pos::new(HPos::Left, VPos::Top));
    chart.draw_series(std::iter::once(Text::new(
        "Error Rate Thresholds",
        (legend_x, legend_y_start),
        title_style,
    )))?;

    // Draw each threshold entry (sorted green to orange in THRESHOLDS)
    for (i, &(threshold, label, color)) in THRESHOLDS.iter().enumerate() {
        let y_pos = legend_y_start - line_height * (i as f64 + 1.2);

        // Draw colored line segment
        let line_style = ShapeStyle {
            color: color.to_rgba(),
            filled: false,
            stroke_width: 2,
        };
        chart.draw_series(LineSeries::new(
            vec![(legend_x, y_pos), (legend_x + line_width, y_pos)],
            line_style,
        ))?;

        // Draw label text
        let label_text = format!("< {}% {}", threshold, label);
        let text_style = TextStyle::from(("sans-serif", 11).into_font())
            .color(&BLACK)
            .pos(Pos::new(HPos::Left, VPos::Center));
        chart.draw_series(std::iter::once(Text::new(
            label_text,
            (legend_x + line_width + x_max * 0.008, y_pos),
            text_style,
        )))?;
    }

    root.present()?;

    Ok(())
}

/// Calculate the axis ranges from all results
fn calculate_ranges(url_results: &[UrlBenchmarkResults]) -> (f64, f64, f64) {
    let mut min_rate = f64::MAX;
    let mut max_rate = 0f64;
    let mut max_error_rate = 0f64;

    for url_result in url_results {
        for result in &url_result.results {
            if result.actual_rate > 0.0 {
                min_rate = min_rate.min(result.actual_rate);
                max_rate = max_rate.max(result.actual_rate);
            }
            max_error_rate = max_error_rate.max(result.error_rate);
        }
    }

    if min_rate == f64::MAX {
        min_rate = 0.0;
    }

    (min_rate, max_rate, max_error_rate)
}

/// Shorten a URL for legend display
fn shorten_url(url: &str) -> String {
    let url = url
        .trim_start_matches("https://")
        .trim_start_matches("http://");

    // Remove trailing slash
    let url = url.trim_end_matches('/');

    // If URL is still too long, truncate with ellipsis
    if url.len() > 40 {
        format!("{}...", &url[..37])
    } else {
        url.to_string()
    }
}
