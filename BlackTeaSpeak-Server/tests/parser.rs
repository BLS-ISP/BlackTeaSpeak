use blackteaspeak_server::query::{
    QueryResponse, decode_query_value, parse_request_line, render_response,
};

#[test]
fn parses_positional_login() {
    let request =
        parse_request_line("login serveradmin serveradmin").expect("request should parse");

    assert_eq!(request.command, "login");
    assert_eq!(request.positional_args, vec!["serveradmin", "serveradmin"]);
    assert!(request.named_args.is_empty());
    assert!(request.option_groups.is_empty());
    assert!(request.flags.is_empty());
}

#[test]
fn parses_named_args_and_flags() {
    let request = parse_request_line("use sid=1 -virtual").expect("request should parse");

    assert_eq!(request.command, "use");
    assert_eq!(request.named_args.get("sid").map(String::as_str), Some("1"));
    assert_eq!(request.option_groups.len(), 1);
    assert_eq!(
        request.option_groups[0].get("sid").map(String::as_str),
        Some("1")
    );
    assert!(request.flags.contains("virtual"));
}

#[test]
fn parses_pipe_separated_option_groups() {
    let request = parse_request_line(
        "clientaddperm cldbid=16 permsid=i_client_move_power permvalue=50 permskip=1|permsid=i_client_poke_power permvalue=20 permskip=0",
    )
    .expect("request should parse");

    assert_eq!(request.command, "clientaddperm");
    assert_eq!(
        request.named_args.get("cldbid").map(String::as_str),
        Some("16")
    );
    assert_eq!(request.option_groups.len(), 2);
    assert_eq!(
        request.option_groups[0].get("cldbid").map(String::as_str),
        Some("16")
    );
    assert_eq!(
        request.option_groups[0].get("permsid").map(String::as_str),
        Some("i_client_move_power")
    );
    assert_eq!(
        request.option_groups[1].get("permsid").map(String::as_str),
        Some("i_client_poke_power")
    );
    assert_eq!(
        request.option_groups[1]
            .get("permvalue")
            .map(String::as_str),
        Some("20")
    );
}

#[test]
fn decodes_escaped_values() {
    assert_eq!(decode_query_value("Default\\sChannel"), "Default Channel");
    assert_eq!(decode_query_value("hello\\pworld"), "hello|world");
}

#[test]
fn renders_bulk_and_success_line() {
    let row = [(
        String::from("channel_name"),
        String::from("Default Channel"),
    )]
    .into_iter()
    .collect();
    let rendered = render_response(&QueryResponse::ok_row(row));

    assert!(rendered.contains("channel_name=Default\\sChannel"));
    assert!(rendered.ends_with("error id=0 msg=ok"));
}
