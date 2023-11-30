//use llm_chain::{executor, parameters, prompt, step::Step};
use aws_sdk_dynamodb::{Client, Error as DynamoError};
use aws_sdk_dynamodb::types::AttributeValue;
use lambda_runtime::{run, service_fn, Error, LambdaEvent};
use serde::{Deserialize, Serialize};
use aws_config::BehaviorVersion;
use anyhow::anyhow;

#[derive(Deserialize)]
struct Request {
    body: String,
}

#[derive(Serialize)]
struct Response {
    statusCode: i32,
    body: String,
}

async fn function_handler(event: LambdaEvent<Request>) -> Result<Response, Error> {
    let request = event.payload;

    // Initialize DynamoDB client
    let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = Client::new(&config);

    // Query DynamoDB
    match query_dynamodb(&client, &request.body).await {
        Ok(Some(res)) => {
            Ok(Response {
                statusCode: 200,
                body: res.to_string(),
            })
        },
        Ok(None) => {
            Ok(Response {
                statusCode: 200,
                //fixme: add logic to call gpt api
                body: transform_result(generate_response(&request.body).await),
            })
        },
        Err(e) => {
            Ok(Response {
                statusCode: 200,
                body: e.to_string(),
            })
            //Err(anyhow!("Error querying DynamoDB").into())
        },
    }
}

fn transform_result(result: Result<String, Box<dyn std::error::Error>>) -> String {
    match result {
        Ok(str_ref) => str_ref.to_string(),
        Err(_) => "Error generating response.".to_string(),
    }
}

async fn generate_response(input: &str) -> Result<String, Box<dyn std::error::Error>> {
    // let exec = executor!()?;
    // let step = Step::for_prompt_template(prompt!(
    //     "You are an experienced travel agent that will provide an in-depth itinerary based on relevant online articles. You will provide the itinerary based on the location and duration entered by the user. Include at least 3 activities a day. Do not include any other suggestions or comments before or after the itinerary.",
    //     "{{text}}" // Text is the default parameter name
    // ));
    // let res = step.run(&parameters!(input), &exec).await?;
    // let itinerary_advice: String = res.to_string();
    // Ok(itinerary_advice)
    // fixme: suspect that gpt's api messes up the environment
    Ok("need to generate response and stuff".to_string())
}

async fn query_dynamodb(client: &Client, input: &str) -> Result<Option<String>, DynamoError> {
    //let input_attr = AttributeValue::S(input.to_string());

    let resp = client.get_item()
        .table_name("WonderNAV-Chats")
        .key("input", AttributeValue::S(input.to_string())) // Pass the key name and value directly
        .send()
        .await?;

    return if let Some(item) = resp.item {
        if let Some(output_attr) = item.get("output") {
            match output_attr.as_s() {
                Ok(output) => Ok(Some(output.to_string())),
                Err(_) => Ok(None)

                //Err(_) => Ok(Some("Item attribute does not exist".to_string()))
            }
        } else {
            Ok(None)
        }
    } else {
        Ok(None)
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .without_time()
        .init();

    run(service_fn(function_handler)).await
}