# Notify Bot
A general purposed bot that send messages to specified users and groups by Webhooks.  

It's supported to extract contents from the webhook request with `json` payloads, format them, then send the processed message to specified users and groups by onebot protocol.

## Usage
> ![NOTE]
> Currently the Bot is under development and not ready for production use.  
> There's no official artifacts yet, so you need to build it yourself.
> 
> ``` bash
> git clone https://github.com/Hamster5295/notify-bot.git
> cd notify-bot
> cargo build --release
> ```

The Notify-Bot can be run by providing a `config.json`.  
``` bash
notify-bot --config /path/to/config.json

# Or if config.json is within the same path of the executable
notify-bot
```

And there you go!  
By configuring the `config.json` file as descripted below, you can specify the url to be hooked by webhook providers, and the how messages will be sent.

## Configuration
The configuration file is written in json.  
Here's the schema:  

``` json
{
    "server": {
        // Specify the ip and port to be listened on
        "ip": "0.0.0.0",
        "port": 10000
    },
    "onebot": {
        // Specify the url of the onebot server
        // Notify-Bot itself does not offer onebot implementation.
        // Checkout https://github.com/botuniverse/onebot for more details.
        "url": "http://127.0.0.1:3000"
    },
    "notifications": [
        {
            // Specify the hook url by setting the id.
            // The hook url will be set to http://your-own-domain.com/notify-{id} where {id} is the value below.
            // In the example below, the url is http://your-own-domain.com/notify-my-server
            "id": "my-server",

            // OPTIONAL. Specify the Bearer Token to be used for authentication.
            "token": "fake-token"

            // OPTIONAL. Specify the groups you'd like to send message to.
            "groups": [
                "123456789"
            ],

            // OPTIONAL. Specify the users you'd like to send message to.
            // Note: If neither the "groups" nor the "users" is specified, nothing would happen.
            "users": [
                "123456789"
            ],

            // The message to be sent.
            // If you'd like to include contents that are posted through request body, use {varible-name}. 
            // Take the configure below as an example.
            "message": "Hello, {user-name}!",

            // OPTIONAL. Specify the user to be mentioned, a.k.a. AT.
            // Take effect only if the message is sent to a group.
            "mention": [
                "1145141919810"
            ],

            // OPTIONAL. If you'd like to include contents that are posted through request body, set it to TRUE. Otherwise, the request body will be ignored.
            "extra": true,

            // OPTIONAL. Specify how to extract contents from the json body of the request.
            "extractors": [
                {
                    // The varible name coresponding to the name used at the "message" field.
                    "name": "user-name",

                    // The path in the json to be extracted.
                    // In this example, the request body is expected to be:
                    // { "sender": { "name": "xxx", ... }, ... }
                    "path": ["sender.name"],

                    // OPTIONAL. Specify the fallback value if the path is not found.
                    "fallback": "User",

                    // OPTIONAL. If there're arrays in the path, the sep will be used to join the elements.
                    // Check the tests at the bottom of src/service.rs for more info.
                    "sep": ","
                }
            ]
        }
    ]
}
```