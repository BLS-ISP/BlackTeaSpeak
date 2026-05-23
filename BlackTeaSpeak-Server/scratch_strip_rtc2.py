import re

with open(r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\web_transport.rs", "r", encoding="utf-8") as f:
    content = f.read()

# 1. Remove impl BlackTeaWebRtcNotifier for BlackTeaWebRtcNotificationBridge
content = re.sub(r"impl BlackTeaWebRtcNotifier for BlackTeaWebRtcNotificationBridge \{.*?\}\n", "", content, flags=re.DOTALL)

# 2. Remove session.set_rtc_manager(rtc_manager);
content = re.sub(r"^\s*session\.set_rtc_manager\(rtc_manager\);\n", "", content, flags=re.MULTILINE)

# 3. Remove rtc_manager creation and rtc_btea_rx loop in BlackTeaWebTransportServer::bind
content = re.sub(r"\s*let rtc_manager = Arc::new\(BlackTeaWebRtcManager::new.*?\);\n", "\n", content, flags=re.DOTALL)
content = re.sub(r"\s*let \(rtc_btea_tx, mut rtc_btea_rx\) = tokio::sync::mpsc::unbounded_channel\(\);\n", "\n", content)
content = re.sub(r"\s*runtime\.lock\(\)\.unwrap\(\)\.rtc_btea_media_tx = Some\(rtc_btea_tx\);\n", "\n", content)
content = re.sub(r"\s*let rtc_manager_for_btea = rtc_manager\.clone\(\);\n\s*tokio::spawn\(async move \{\n\s*while let Some\(\(server_id, sender_client_id, payload\)\) = rtc_btea_rx\.recv\(\)\.await \{\n\s*rtc_manager_for_btea\.handle_btea_video_packet\(server_id, sender_client_id, &payload\)\.await;\n\s*\}\n\s*\}\);\n", "\n", content)

# 4. Remove parse_video_broadcast_options usages and the publish_video / modify_video blocks
# Look for "let video_options = match parse_video_broadcast_options"
content = re.sub(r"\s*let video_options = match parse_video_broadcast_options\(row\) \{.*?\}\n", "\n", content, flags=re.DOTALL)

# There is probably a usage of video_options right after it
content = re.sub(r"\s*if let Some\(mgr\) = &self\.rtc_manager \{.*?\}\n", "\n", content, flags=re.DOTALL)

with open(r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\web_transport.rs", "w", encoding="utf-8") as f:
    f.write(content)
