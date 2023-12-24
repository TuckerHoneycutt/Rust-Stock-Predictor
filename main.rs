#[macro_use]
extern crate prettytable;

use yahoo_finance_api as yahoo;
use std::time::{Duration, UNIX_EPOCH};
use tokio_test;
use prettytable::{Table, Row, Cell};
use chrono::{DateTime, Utc, NaiveDate, Datelike, NaiveDateTime, Local, TimeZone, LocalResult};
use num_format::{Locale, ToFormattedString};
use dialoguer::{Input, Select};
use csv::Writer;
use std::fs::File;
use plotters::prelude::*;

fn main() {
    let today = Utc::today().naive_utc();
    // Prompt the user for the stock symbol
    let stock_symbol: String = Input::new()
        .with_prompt("Enter the stock symbol (e.g., MSFT)")
        .interact_text()
        .expect("Error getting stock symbol");

    let mut csv_writer = Writer::from_writer(File::create("stock_data.csv").expect("Error creating CSV file"));

    // Prompt the user for the start date
    let start_date_str: String = Input::new()
        .with_prompt("Enter the start date (YYYY-MM-DD)")
        .interact_text()
        .expect("Error getting start date");

    let start_date = chrono::NaiveDate::parse_from_str(&start_date_str, "%Y-%m-%d")
        .expect("Error parsing start date");

    let start_timestamp = start_date.and_hms(0, 0, 0).timestamp() as u64;

    // Fetch stock data from the start date until today
    let provider = yahoo::YahooConnector::new();
    let response = tokio_test::block_on(provider.get_quote_range(&stock_symbol, "1d", &format!("120mo")));

    match response {
        Ok(response) => {
            let quotes = response.quotes().unwrap_or_else(|err| {
                eprintln!("Error getting quotes: {:?}", err);
                Vec::new()
            });

            // Initialize variables to track current month and year
            let mut current_month = 0;
            let mut current_year = 0;

            // Vectors to store closing prices and their corresponding timestamps
            let mut closing_prices = Vec::new();
            let mut timestamps = Vec::new();

            // Iterate through quotes and create tables for each month
            for quote in quotes.iter() {
                let (quote_month, quote_year) = get_month_and_year(&quote.timestamp);

                // Check if the month has changed
                if quote_month != current_month || quote_year != current_year {
                    // Create a new table for the month
                    let mut table = Table::new();
                    table.add_row(row!["Date", "Open", "High", "Low", "Volume", "Close", "AdjClose"]);

                    // Add rows to the table for the current month
                    for month_quote in quotes.iter().filter(|&q| {
                        let (q_month, q_year) = get_month_and_year(&q.timestamp);
                        q_month == quote_month && q_year == quote_year
                    }) {
                        let formatted_date = format_date(&month_quote.timestamp);
                        let formatted_volume = month_quote.volume.to_formatted_string(&Locale::en);

                        // Add the data row
                        table.add_row(Row::new(vec![
                            Cell::new(&formatted_date),
                            Cell::new(&format!("{:.2}", month_quote.open)),
                            Cell::new(&format!("{:.2}", month_quote.high)),
                            Cell::new(&format!("{:.2}", month_quote.low)),
                            Cell::new(&formatted_volume),
                            Cell::new(&format!("{:.2}", month_quote.close)),
                            Cell::new(&format!("{:.2}", month_quote.adjclose)),
                        ]));

                        // Write the data row to the CSV file
                        csv_writer.write_record(&[
                            formatted_date,
                            format!("{:.2}", month_quote.open),
                            format!("{:.2}", month_quote.high),
                            format!("{:.2}", month_quote.low),
                            format!("{:.2}", month_quote.volume),
                            format!("{:.2}", month_quote.close),
                            format!("{:.2}", month_quote.adjclose),
                        ]).expect("Error writing to CSV file");

                        // Store closing prices and timestamps for plotting
                        closing_prices.push(month_quote.close);
                        timestamps.push(month_quote.timestamp);
                    }

                    // Print the table for the current month
                    println!(
                        "\n{} {}\n",
                        month_number_to_name(quote_month),
                        quote_year
                    );
                    table.printstd();

                    // Update the current month and year
                    current_month = quote_month;
                    current_year = quote_year;
                }
            }

            // Vectors to store OHLC values and their corresponding timestamps
            let mut opens = Vec::new();
            let mut highs = Vec::new();
            let mut lows = Vec::new();
            let mut closes = Vec::new();
            let mut timestamps_candlestick = Vec::new(); // Rename to avoid duplication

            for quote in quotes.iter() {
                opens.push(quote.open);
                highs.push(quote.high);
                lows.push(quote.low);
                closes.push(quote.close);
                timestamps_candlestick.push(quote.timestamp); // Rename to avoid duplication
            }

            // Plot candlestick chart
            if let Err(e) = plot_candlestick_chart(&stock_symbol, &timestamps_candlestick, &opens, &highs, &lows, &closes) {
                eprintln!("Error plotting candlestick chart: {:?}", e);
            }
        }
        Err(err) => {
            eprintln!("Error fetching stock data: {:?}", err);
        }
    }
}

fn format_date(timestamp: &u64) -> String {
    let naive_datetime = chrono::NaiveDateTime::from_timestamp(*timestamp as i64, 0);
    let datetime: DateTime<Utc> = DateTime::from_utc(naive_datetime, Utc);
    datetime.format("%m-%d-%Y").to_string()
}

fn get_month_and_year(timestamp: &u64) -> (u32, i32) {
    let naive_datetime = chrono::NaiveDateTime::from_timestamp(*timestamp as i64, 0);
    (naive_datetime.month(), naive_datetime.year())
}

fn month_number_to_name(month: u32) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "",
    }
}

fn plot_candlestick_chart(
    stock_symbol: &str,
    dates: &Vec<u64>,
    opens: &Vec<f64>,
    highs: &Vec<f64>,
    lows: &Vec<f64>,
    closes: &Vec<f64>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create a plotter backend
    let root = BitMapBackend::new("candlestick_chart.png", (1920, 1080)).into_drawing_area();
    root.fill(&WHITE)?;

    // Determine the start and end timestamps from your dataset
    let start_timestamp = *dates.first().unwrap_or(&0);
    let end_timestamp = *dates.last().unwrap_or(&0);

    // Create a chart
    let mut chart = ChartBuilder::on(&root)
        .caption(format!("{} Stock Chart", stock_symbol), ("Arial", 30).into_font())  // Set dynamic chart title
        .set_label_area_size(LabelAreaPosition::Left, 50)
        .set_label_area_size(LabelAreaPosition::Bottom, 50)
        .margin(5)
        .build_cartesian_2d(
            start_timestamp as f64..end_timestamp as f64,  // Use your dataset's timestamps here
            lows.iter().cloned().fold(f64::NAN, f64::min)..highs.iter().cloned().fold(f64::NAN, f64::max),  // Adjust based on your price values
        )?;

    // Draw the candlestick chart
    chart.configure_mesh().x_labels(5).draw()?;  // Set the number of x-axis labels
    chart.configure_mesh().draw()?;
    chart.draw_series(
        dates.iter().zip(opens.iter().zip(highs.iter().zip(lows.iter().zip(closes.iter()))))
            .map(|(date, (open, (high, (low, close))))| {
                (date.clone() as f64, *open, *high, *low, *close, &GREEN, &RED, 1.0)
            })
            .map(|(date, open, high, low, close, color_up, color_down, candle_width)| {
                CandleStick::new(date, open, high, low, close, color_up, color_down, candle_width as u32)
            })
    )?;

// Format x-axis labels as "YYYY-MM-DD" in local time
chart.configure_mesh()
    .x_label_formatter(&|timestamp| {
        let naive_datetime = NaiveDateTime::from_timestamp(*timestamp as i64, 0);
        match Local.from_local_datetime(&naive_datetime) {
            LocalResult::Single(datetime_local) => datetime_local.format("%Y-%m-%d").to_string(),
            LocalResult::None | LocalResult::Ambiguous(_, _) => String::from("Invalid Date"),
        }
    })
    .x_labels(10) // Set the number of x-axis labels
    .draw()?;
    Ok(())
}
