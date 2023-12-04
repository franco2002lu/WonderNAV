//! # Chat Interaction Handler for AWS Lambda
//!
//! This module provides functionalities for handling chat interactions in an AWS Lambda environment.
//! It primarily focuses on processing HTTP requests and generating appropriate responses,
//! leveraging AWS services like DynamoDB and external APIs such as OpenAI.
use async_openai::{
    config::OpenAIConfig, types::CreateCompletionRequestArgs, Client as OpenAIClient,
};
use aws_config::BehaviorVersion;
use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::{Client, Error as DynamoError};
use lambda_runtime::{run, service_fn, Error, LambdaEvent};
use serde::{Deserialize, Serialize};

/// Represents an HTTP request.
///
/// This struct is used to deserialize incoming HTTP requests.
/// It focuses on capturing the body of the request as a `String`.
///
/// # Attributes
/// * `body` - A `String` containing the body of the HTTP request.
#[derive(Deserialize)]
struct Request {
    body: String,
}

/// Represents an HTTP response.
///
/// This struct is used to serialize data into an HTTP response format.
/// It includes both a status code and a response body.
///
/// Note: The `statusCode` field uses camelCase as specified by AWS standards.
///
/// # Attributes
/// * `statusCode` - An `i32` representing the HTTP status code of the response.
/// * `body` - A `String` containing the body of the HTTP response.
#[derive(Serialize)]
struct Response {
    statusCode: i32, // AWS specifies for me to use camelCase for statusCode here
    body: String,
}

/// Asynchronous AWS Lambda function handler for processing requests.
///
/// This function serves as an AWS Lambda handler. It takes an `event` of type `LambdaEvent<Request>`
/// and processes it to generate a response. The function queries a DynamoDB table with the request body.
/// If a matching record is found, it returns the record's data. If not, it generates a new response
/// using an external API, stores the result in DynamoDB, and then returns it.
///
/// # Arguments
///
/// * `event` - A `LambdaEvent<Request>` object representing the AWS Lambda event. It contains
///   the request payload.
///
/// # Returns
///
/// Returns a `Result<Response, Error>`. On successful processing of the event, it returns `Ok(Response)`
/// where `Response` contains the HTTP status code and the body of the response. In the event of an error
/// (such as a failure to query DynamoDB), it returns `Ok(Response)` with a status code of 500 and an error message.
///
/// # Errors
///
/// Errors can arise from:
///
/// - Failures in querying DynamoDB.
/// - Failures in generating a response using the external API.
/// - Failures in putting a new item into the DynamoDB table.
///
/// In case of any error, the function returns a `Response` with a status code of 500 and an error message.
async fn function_handler(event: LambdaEvent<Request>) -> Result<Response, Error> {
    let request = event.payload;

    // Initialize DynamoDB client
    let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = Client::new(&config);

    // Query DynamoDB
    match query_dynamodb(&client, &request.body).await {
        Ok(Some(res)) => Ok(Response {
            statusCode: 200,
            body: res.to_string(),
        }),
        Ok(None) => {
            let openai_resp = transform_result(generate_response(&request.body).await);
            let _put_response = client
                .put_item()
                .table_name("WonderNAV-Chats")
                .item("input", AttributeValue::S(request.body.clone()))
                .item("output", AttributeValue::S(openai_resp.clone()))
                .send()
                .await?;
            Ok(Response {
                statusCode: 200,
                body: openai_resp,
            })
        }
        Err(e) => Ok(Response {
            statusCode: 500,
            body: format!("Error querying DynamoDB: {}", e),
        }),
    }
}

/// Transforms a `Result` into a `String`, providing a default error message on failure.
///
/// This function is designed to handle the output from functions that return a `Result<String, Box<dyn std::error::Error>>`.
/// It simplifies the error handling by converting any `Err` variant into a generic error message string.
///
/// # Arguments
///
/// * `result` - A `Result` object which may contain either a `String` or a `Box<dyn std::error::Error>`.
///
/// # Returns
///
/// Returns a `String`. If `result` is `Ok`, it returns the contained `String`. If `result` is `Err`, it returns
/// a default error message: `"Error generating response."`.
fn transform_result(result: Result<String, Box<dyn std::error::Error>>) -> String {
    match result {
        Ok(str_ref) => str_ref.to_string(),
        Err(_) => "Error generating response.".to_string(),
    }
}

/// Generates a travel itinerary based on the provided input using the OpenAI API.
///
/// This asynchronous function sends a request to the OpenAI API to generate a detailed travel itinerary.
/// The request includes a predefined prompt to which the user's input is appended. The function
/// then parses the response to extract the generated itinerary.
///
/// # Arguments
///
/// * `input` - A string slice that represents the user's input. This should typically include
///   the location and duration of the intended trip.
///
/// # Returns
///
/// This function returns a `Result` type. On success, it returns `Ok(String)` containing
/// the generated itinerary. In the event of an error (such as a problem with the API request),
/// it returns an `Err` with a boxed `dyn std::error::Error`.
///
/// # Errors
///
/// This function will return an error in several cases, including:
///
/// - Problems with the network connectivity.
/// - Errors from the OpenAI API (e.g., invalid API key, API limitations).
/// - Issues with building the OpenAI API request.
async fn generate_response(input: &str) -> Result<String, Box<dyn std::error::Error>> {
    let api_key = "api key placeholder"; //todo: api key here
    let openai_config = OpenAIConfig::new().with_api_key(api_key);

    let openai_client = OpenAIClient::with_config(openai_config);
    let mut openai_prompt = "You are an experienced travel agent that will provide an in-depth itinerary based on relevant online articles. You will provide the itinerary based on the location and duration entered by the user. Include at least 3 activities a day. Do not include any other suggestions or comments before or after the itinerary."
        .to_string();
    openai_prompt.push_str(input);

    let openai_request = CreateCompletionRequestArgs::default()
        .model("text-davinci-003")
        .prompt(openai_prompt)
        .max_tokens(900_u16)
        .build()
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

    let openai_response = openai_client
        .completions()
        .create(openai_request)
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

    let output = &openai_response.choices[0].text;
    Ok(output.to_string())
}

/// Queries a DynamoDB table for a specific item.
///
/// This asynchronous function takes a reference to a DynamoDB `Client` and a string `input`.
/// It queries the DynamoDB table "WonderNAV-Chats" for an item with a key matching the `input`.
///
/// # Arguments
///
/// * `client` - A reference to the DynamoDB `Client` used to perform the query.
/// * `input` - A string slice that represents the key of the item to query in the DynamoDB table.
///
/// # Returns
///
/// This function returns a `Result` type. On success, it returns `Ok(Some(String))` where the `String`
/// is the value corresponding to the 'output' attribute of the item found in the table.
/// If the 'output' attribute is not found or the item does not exist in the table,
/// it returns `Ok(None)`.
///
/// In the case of an error during the query (e.g., network issues, permissions problems),
/// it returns an `Err(DynamoError)`.
///
/// # Errors
///
/// This function will return an error if the DynamoDB query fails for reasons such as
/// network issues, incorrect permissions, or invalid input format.
async fn query_dynamodb(client: &Client, input: &str) -> Result<Option<String>, DynamoError> {
    let resp = client
        .get_item()
        .table_name("WonderNAV-Chats")
        .key("input", AttributeValue::S(input.to_string()))
        .send()
        .await?;

    return if let Some(item) = resp.item {
        if let Some(output_attr) = item.get("output") {
            match output_attr.as_s() {
                Ok(output) => Ok(Some(output.to_string())),
                Err(_) => Ok(Some("Item attribute does not exist".to_string())),
            }
        } else {
            Ok(None)
        }
    } else {
        Ok(None)
    };
}

/// The entry point of the application.
///
/// This asynchronous function sets up the logging infrastructure and then runs the service.
/// It uses Tokio's asynchronous runtime to drive the application and `tracing_subscriber`
/// to set up logging. The `function_handler` is passed to the service runner.
///
/// # Returns
///
/// Returns `Result<(), Error>`. If the application runs successfully and exits gracefully,
/// it returns `Ok(())`. If there is an error in setting up the service or during its runtime,
/// it returns `Err(Error)`.
///
/// # Panics
///
/// This function may panic if the Tokio runtime fails to initialize or if there is an unrecoverable error
/// in the service setup or execution.
#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .without_time()
        .init();

    run(service_fn(function_handler)).await
}
