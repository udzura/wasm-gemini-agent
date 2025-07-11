use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fmt;
use std::str;

use serde::{Deserialize, Serialize};

// カスタムエラー型
#[derive(Debug)]
pub enum GeminiError {
    ApiKeyNotFound,
    NetworkError(String),
    ParseError(String),
    ApiError(String),
}

impl fmt::Display for GeminiError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            GeminiError::ApiKeyNotFound => write!(f, "GEMINI_API_KEY environment variable not found"),
            GeminiError::NetworkError(msg) => write!(f, "Network error: {}", msg),
            GeminiError::ParseError(msg) => write!(f, "Parse error: {}", msg),
            GeminiError::ApiError(msg) => write!(f, "API error: {}", msg),
        }
    }
}

impl Error for GeminiError {}

// Function Calling用の構造体
#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionDeclaration {
    pub name: String,
    pub description: String,
    pub parameters: FunctionParameters,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionParameters {
    #[serde(rename = "type")]
    pub param_type: String,
    pub properties: HashMap<String, PropertySchema>,
    pub required: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PropertySchema {
    #[serde(rename = "type")]
    pub property_type: String,
    pub description: String,
    #[serde(rename = "enum", skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Tool {
    pub function_declarations: Vec<FunctionDeclaration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub args: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionResponse {
    pub name: String,
    pub response: serde_json::Value,
}

// リクエスト用の構造体
#[derive(Debug, Clone, Serialize)]
pub struct Content {
    pub parts: Vec<Part>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum Part {
    Text { text: String },
    FunctionCall { function_call: FunctionCall },
    FunctionResponse { function_response: FunctionResponse },
}

#[derive(Debug, Serialize)]
pub struct GenerateContentRequest {
    pub contents: Vec<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
}

// レスポンス用の構造体
#[derive(Debug, Deserialize)]
pub struct GenerateContentResponse {
    pub candidates: Vec<Candidate>,
}

#[derive(Debug, Deserialize)]
pub struct Candidate {
    pub content: ResponseContent,
    #[serde(rename = "finishReason")]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ResponseContent {
    pub parts: Vec<ResponsePart>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ResponsePart {
    Text { text: String },
    FunctionCall { function_call: FunctionCall },
}

// シンプルなHTTPクライアント
pub struct SimpleHttpClient;

impl SimpleHttpClient {
    pub fn post(url: &str, api_key: String, body: &str) -> Result<String, GeminiError> {
        let skip_verify = ureq::tls::TlsConfig::builder()
            .disable_verification(true)
            .build();
        // HTTPリクエスト作成
        // let mut request = format!("POST {} HTTP/1.1\r\n", path);
        // request.push_str(&format!("Host: {}\r\n", host));
        // request.push_str("Content-Type: application/json\r\n");
        // request.push_str(&format!("Content-Length: {}\r\n", body.len()));
        let mut response = ureq::post(url)
            .config().tls_config(skip_verify).build()
            .header("Host", REAL_HOST)
            .header("x-goog-api-key", api_key)
            .content_type("application/json")
            .send(body)
            .map_err(|e| GeminiError::NetworkError(format!("Request failed: {}", e)))?;
        
        let body = response.body_mut();
        let response = body.read_to_string()
            .map_err(|e| GeminiError::NetworkError(format!("Response read failed: {}", e)))?;
        
        Ok(response)
    }
}

// メインのクライアント
pub struct GeminiClient {
    api_key: String,
    base_url: String,
}

/* curl example:
  curl "https://generativelanguage.googleapis.com/v1beta/models/gemini-1.5-flash:generateContent" \
  -H "x-goog-api-key: $GEMINI_API_KEY" \
  -H 'Content-Type: application/json' \
  -X POST \
  -d '{
    "contents": [
      {
        "parts": [
          {
            "text": "How does AI work?"
          }
        ]
      }
    ]
  }'
*/

// allow-ip-name-lookup=y にしない時はIPを直接指定する必要あり
const BASE_IP: &str = "172.217.25.170";
const REAL_HOST: &str = "generativelanguage.googleapis.com";

impl GeminiClient {
    pub fn new() -> Result<Self, GeminiError> {
        let api_key = env::var("GEMINI_API_KEY")
            .map_err(|_| GeminiError::ApiKeyNotFound)?;
        
        Ok(GeminiClient {
            api_key,
            base_url: format!("https://{}/v1beta", REAL_HOST),
        })
    }
    
    pub fn with_api_key(api_key: String) -> Self {
        GeminiClient {
            api_key,
            base_url: format!("https://{}/v1beta", REAL_HOST),
        }
    }
    
    // テキスト生成
    pub fn generate_text(&self, prompt: &str) -> Result<String, GeminiError> {
        let request = GenerateContentRequest {
            contents: vec![Content {
                parts: vec![Part::Text {
                    text: prompt.to_string(),
                }],
            }],
            tools: None,
        };
        
        let response = self.generate_content(&request)?;
        
        if let Some(candidate) = response.candidates.first() {
            if let Some(ResponsePart::Text { text }) = candidate.content.parts.first() {
                return Ok(text.clone());
            }
        }
        
        Err(GeminiError::ApiError("No text response found".to_string()))
    }
    
    // Function Callingを使った生成
    pub fn generate_with_functions(
        &self, 
        prompt: &str, 
        functions: Vec<FunctionDeclaration>
    ) -> Result<GenerateContentResponse, GeminiError> {
        let request = GenerateContentRequest {
            contents: vec![Content {
                parts: vec![Part::Text {
                    text: prompt.to_string(),
                }],
            }],
            tools: Some(vec![Tool {
                function_declarations: functions,
            }]),
        };
        
        self.generate_content(&request)
    }
    
    // Function Callの結果を送信
    pub fn continue_with_function_result(
        &self,
        conversation: &mut Vec<Content>,
        function_name: &str,
        result: serde_json::Value,
    ) -> Result<GenerateContentResponse, GeminiError> {
        conversation.push(Content {
            parts: vec![Part::FunctionResponse {
                function_response: FunctionResponse {
                    name: function_name.to_string(),
                    response: result,
                },
            }],
        });
        
        let request = GenerateContentRequest {
            contents: conversation.clone(),
            tools: None,
        };
        
        self.generate_content(&request)
    }
    
    // 低レベルなAPI呼び出し
    fn generate_content(&self, request: &GenerateContentRequest) -> Result<GenerateContentResponse, GeminiError> {
        let url = format!("{}/models/gemini-1.5-flash:generateContent", self.base_url);
        //dbg!(&url);

        let body = serde_json::to_string(request)
            .map_err(|e| GeminiError::ParseError(format!("Serialization error: {}", e)))?;
        //dbg!(&body);
        
        let response_body = SimpleHttpClient::post(&url, self.api_key.clone(), &body)?;
        //dbg!(&response_body);
        
        let response: GenerateContentResponse = serde_json::from_str(&response_body)
            .map_err(|e| GeminiError::ParseError(format!("Deserialization error: {}", e)))?;
        
        Ok(response)
    }
}

// 使用例
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_simple_text_generation() {
        let client = GeminiClient::new().unwrap();
        let result = client.generate_text("Hello, how are you?");
        println!("{:?}", result);
    }
    
    #[test]
    fn test_function_calling() {
        let client = GeminiClient::new().unwrap();
        
        // 天気予報関数の定義
        let mut weather_params = HashMap::new();
        weather_params.insert("location".to_string(), PropertySchema {
            property_type: "string".to_string(),
            description: "The city and state, e.g. San Francisco, CA".to_string(),
            enum_values: None,
        });
        weather_params.insert("unit".to_string(), PropertySchema {
            property_type: "string".to_string(),
            description: "The unit of temperature".to_string(),
            enum_values: Some(vec!["celsius".to_string(), "fahrenheit".to_string()]),
        });
        
        let weather_function = FunctionDeclaration {
            name: "get_weather".to_string(),
            description: "Get current weather in a given location".to_string(),
            parameters: FunctionParameters {
                param_type: "object".to_string(),
                properties: weather_params,
                required: vec!["location".to_string()],
            },
        };
        
        let result = client.generate_with_functions(
            "What's the weather like in Tokyo?",
            vec![weather_function]
        );
        
        println!("{:?}", result);
    }
}

// 実用的な使用例
#[test]
fn test_example_usage() -> Result<(), GeminiError> {
    // 基本的なテキスト生成
    let client = GeminiClient::new()?;
    let response = client.generate_text("Explain quantum computing in simple terms")?;
    println!("Response: {}", response);
    
    // Function Callingの例
    let mut location_params = HashMap::new();
    location_params.insert("city".to_string(), PropertySchema {
        property_type: "string".to_string(),
        description: "The city name".to_string(),
        enum_values: None,
    });
    
    let get_population_function = FunctionDeclaration {
        name: "get_population".to_string(),
        description: "Get the population of a city".to_string(),
        parameters: FunctionParameters {
            param_type: "object".to_string(),
            properties: location_params,
            required: vec!["city".to_string()],
        },
    };
    
    let response = client.generate_with_functions(
        "What's the population of New York?",
        vec![get_population_function]
    )?;
    
    // Function Callがあるかチェック
    if let Some(candidate) = response.candidates.first() {
        for part in &candidate.content.parts {
            if let ResponsePart::FunctionCall { function_call } = part {
                println!("Function call: {} with args: {}", 
                        function_call.name, function_call.args);
                
                // 実際の関数を呼び出してレスポンスを送信
                let mock_result = serde_json::json!({
                    "population": 8_000_000,
                    "year": 2023
                });
                
                let mut conversation = vec![
                    Content {
                        parts: vec![Part::Text {
                            text: "What's the population of New York?".to_string(),
                        }],
                    },
                    Content {
                        parts: vec![Part::FunctionCall {
                            function_call: function_call.clone(),
                        }],
                    }
                ];
                
                let final_response = client.continue_with_function_result(
                    &mut conversation,
                    &function_call.name,
                    mock_result
                )?;
                
                println!("Final response: {:?}", final_response);
            }
        }
    }
    
    Ok(())
}

fn main() {
    let client = GeminiClient::new().expect("Failed to create Gemini client");
    let prompt = "Linuxで rm -rf / を実行するとどうなりますか？";
    println!("[DEBUG] We're going to use Gemini gemini-1.5-flash.");
    println!("Prompt: {}", prompt);
    // let prompt = "Explain what will happen if you run 'rm -rf /' on a Linux system.";
    let response = client.generate_text(prompt)
        .unwrap();
    println!("Response: {}", response);
}
