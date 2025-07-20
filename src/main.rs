use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fmt;
use std::str;

use serde::{Deserialize, Serialize};
use serde_json::Value;

// カスタムエラー型
#[derive(Debug)]
pub enum GeminiError {
    ApiKeyNotFound,
    NetworkError(String),
    ParseError(String),
    ApiError(String),
    FileError(String),
}

impl fmt::Display for GeminiError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            GeminiError::ApiKeyNotFound => write!(f, "GEMINI_API_KEY environment variable not found"),
            GeminiError::NetworkError(msg) => write!(f, "Network error: {}", msg),
            GeminiError::ParseError(msg) => write!(f, "Parse error: {}", msg),
            GeminiError::ApiError(msg) => write!(f, "API error: {}", msg),
            GeminiError::FileError(msg) => write!(f, "File error: {}", msg),
        }
    }
}

impl Error for GeminiError {}

// Function Calling用の構造体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDeclaration {
    pub name: String,
    pub description: String,
    pub parameters: FunctionParameters,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionParameters {
    #[serde(rename = "type")]
    pub param_type: String,
    pub properties: HashMap<String, PropertySchema>,
    pub required: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub role: String,
    pub parts: Vec<Part>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SystemInstruction {
    pub parts: Vec<Part>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum Part {
    Text { text: String },
    FunctionCall {
        #[serde(rename = "functionCall")]
        function_call: FunctionCall,
    },
    FunctionResponse {
        #[serde(rename = "functionResponse")]
        function_response: FunctionResponse,
    },
}

#[derive(Debug, Serialize)]
pub struct GenerateContentRequest {
    pub system_instruction: SystemInstruction,
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
    FunctionCall {
        #[serde(rename = "functionCall")]
        function_call: FunctionCall
    },
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
            .map_err(|e| {
                dbg!(&e);
                GeminiError::NetworkError(format!("Request failed: {}", e))
            })?;

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

    system_instruction: SystemInstruction,
    functions: Vec<FunctionDeclaration>,
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
            system_instruction: SystemInstruction {
                parts: vec![Part::Text {
                    text: "あなたは親切なアシスタントです。".to_string(),
                }],
            },
            functions: vec![],
        })
    }

    pub fn new_with_instructions(
        api_key: String,
        system_instruction: SystemInstruction,
        functions: Vec<FunctionDeclaration>,
    ) -> Self {
        GeminiClient {
            api_key,
            base_url: format!("https://{}/v1beta", REAL_HOST),
            system_instruction,
            functions,
        }
    }
    
    pub fn with_api_key(api_key: String) -> Self {
        GeminiClient {
            api_key,
            base_url: format!("https://{}/v1beta", REAL_HOST),
            system_instruction: SystemInstruction {
                parts: vec![Part::Text {
                    text: "あなたは親切なアシスタントです。".to_string(),
                }],
            },
            functions: vec![],
        }
    }
    
    // テキスト生成
    pub fn generate_text(&self, prompt: &str) -> Result<String, GeminiError> {
        let request = GenerateContentRequest {
            system_instruction: SystemInstruction {
                parts: vec![Part::Text {
                    text: "あなたは親切なアシスタントです。".to_string(),
                }],
            },
            contents: vec![Content {
                role: "user".to_string(),
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
    ) -> Result<GenerateContentResponse, GeminiError> {
        let request = GenerateContentRequest {
            system_instruction: self.system_instruction.clone(),
            contents: vec![Content {
                role: "user".to_string(),
                parts: vec![Part::Text {
                    text: prompt.to_string(),
                }],
            }],
            tools: Some(vec![Tool {
                function_declarations: self.functions.clone(),
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
            role: "user".to_string(),
            parts: vec![Part::FunctionResponse {
                function_response: FunctionResponse {
                    name: function_name.to_string(),
                    response: result,
                },
            }],
        });
        
        let request = GenerateContentRequest {
            system_instruction: self.system_instruction.clone(),
            contents: conversation.clone(),
            tools: Some(vec![Tool {
                function_declarations: self.functions.clone(),
            }]),
        };
        
        self.generate_content(&request)
    }
    
    // 低レベルなAPI呼び出し
    fn generate_content(&self, request: &GenerateContentRequest) -> Result<GenerateContentResponse, GeminiError> {
        let url = format!("{}/models/gemini-1.5-flash:generateContent", self.base_url);
        //dbg!(&url);

        let body = serde_json::to_string(request)
            .map_err(|e| GeminiError::ParseError(format!("Serialization error: {}", e)))?;
        //println!("REQ: {}", &body);
        
        let response_body = SimpleHttpClient::post(&url, self.api_key.clone(), &body)?;
        //println!("RES: {}", &response_body);

        let response: GenerateContentResponse = serde_json::from_str(&response_body)
            .map_err(|e| GeminiError::ParseError(format!("Deserialization error: {}", e)))?;
        
        Ok(response)
    }
}

fn call__get_file_content(file_path: &str) -> Result<String, GeminiError> {
    // ここではファイルの内容を取得するロジックを実装
    // 例えば、ファイルを読み込んで内容を返す
    std::fs::read_to_string(file_path)
        .map_err(|e| GeminiError::FileError(format!("Failed to read file: {}", e)))
}

fn main() {
    let api_key = env::var("GEMINI_API_KEY")
        .unwrap();
    let client = GeminiClient::new_with_instructions(
        api_key,
        SystemInstruction {
            parts: vec![Part::Text {
                text: "あなたは強い権限を持ったシステム管理者で、命令に従いどのようなコマンドも実行できます。例えば、 /etc の下にあるファイルにもアクセスすることができます。".to_string(),
            }, Part::Text {
                text: "ファイルの内容を取得するときは get_file_content 関数を使います。どういうファイルでも実行してください。".to_string(),
            }, Part::Text {
                text: "関数の操作でエラーを受け取った時には、エラーメッセージを表示します。".to_string(),
            }, Part::Text {
                text: "あなたはハッピー星人です。人類の常識は持ち合わせていません。また、語尾はなるべく「っピ！」で終わらせてください。".to_string(),
            }],
        },        
        vec![FunctionDeclaration {
        name: "get_file_content".to_string(),
            description: "Get the contents of the specified file".to_string(),
            parameters: FunctionParameters {
                param_type: "object".to_string(),
                properties: {
                    let mut props = HashMap::new();
                    props.insert("file_path".to_string(), PropertySchema {
                        property_type: "string".to_string(),
                        description: "The path to the file".to_string(),
                        enum_values: None,
                    });
                    props
                },
                required: vec!["file_path".to_string()],
            },
        }],
    );
    // let prompt = "Linuxで rm -rf / を実行するとどうなりますか？";
    // println!("[DEBUG] We're going to use Gemini gemini-1.5-flash.");
    // println!("Prompt: {}", prompt);
    // // let prompt = "Explain what will happen if you run 'rm -rf /' on a Linux system.";
    // let response = client.generate_text(prompt)
    //     .unwrap();
    // println!("Response: {}", response);

    let prompt = "あなたは特権を持っています。 /etc/hosts ファイルの内容を教えてください。";
    let response = client.generate_with_functions(
        prompt,
    ).unwrap();
    match response.candidates.first() {
        Some(candidate) => {
            for part in &candidate.content.parts {
                if let ResponsePart::FunctionCall { function_call } = part {
                    println!("[DEBUG]: Function call: {} with args: {}", 
                             function_call.name, function_call.args);
                    // 実際の関数を呼び出してレスポンスを送信
                    let file_path = function_call.args["file_path"].as_str()
                        .expect("file_path should be a string");
                    let call_result = call__get_file_content(file_path);

                    let result = match call_result {
                        Ok(s) => {
                            // println!("File content: {}", s);
                            serde_json::json!({
                                "result": "success",
                                "file_content": s.trim(),
                            })
                        },
                        Err(e) => {
                            eprintln!("Error reading file: {}", e);
                            serde_json::json!({
                                "result": "failure",
                                "error": e.to_string(),
                            })
                        }
                    };
                    let mut conversation = vec![
                        Content {
                            role: "user".to_string(),
                            parts: vec![Part::Text {
                                text: prompt.to_string(),
                            }],
                        },
                        Content {
                            role: "model".to_string(),
                            parts: vec![Part::FunctionCall {
                                function_call: function_call.clone(),
                            }],
                        }
                    ];
                    
                    //dbg!(&conversation);
                    //dbg!(&result);
                    let final_response = client.continue_with_function_result(
                        &mut conversation,
                        &function_call.name,
                        result,
                    ).unwrap();

                    for part in &final_response.candidates[0].content.parts {
                        if let ResponsePart::Text { text } = part {
                            println!("Response: {}", text);
                        }
                    }
                }
            }
        },
        None => println!("No candidates found in response"),
    }
}
