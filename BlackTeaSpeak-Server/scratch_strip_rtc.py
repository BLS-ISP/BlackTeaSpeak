import re

with open(r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\web_transport.rs", "r", encoding="utf-8") as f:
    content = f.read()

# 1. Remove rtc_manager from BlackTeaWebTransportServer struct
content = re.sub(r"^\s*rtc_manager:\s*Arc<BlackTeaWebRtcManager>,\n", "", content, flags=re.MULTILINE)

# 2. Remove rtc_manager from BlackTeaWebSessionHandler struct
content = re.sub(r"^\s*rtc_manager:\s*Option<Arc<BlackTeaWebRtcManager>>,\n", "", content, flags=re.MULTILINE)

# 3. Remove BlackTeaWebRtcNotificationBridge
content = re.sub(r"pub struct BlackTeaWebRtcNotificationBridge \{.*?\}.*?impl BlackTeaWebRtcNotifier for BlackTeaWebRtcNotificationBridge \{.*?\}\n", "", content, flags=re.DOTALL)

# 4. Remove set_rtc_manager function
content = re.sub(r"^\s*fn set_rtc_manager\(&mut self.*?\}\n", "", content, flags=re.DOTALL | re.MULTILINE)

# 5. Remove rtc_identity function
content = re.sub(r"^\s*fn rtc_identity\(&self\).*?\}\n", "", content, flags=re.DOTALL | re.MULTILINE)

# 6. Remove rtc_manager param from handle_client
content = re.sub(r"^\s*rtc_manager:\s*Arc<BlackTeaWebRtcManager>,\n", "", content, flags=re.MULTILINE)

# 7. Remove VideoBroadcastOptions completely
content = re.sub(r"pub struct VideoBroadcastOptions \{.*?\}", "", content, flags=re.DOTALL)
content = re.sub(r"fn parse_video_broadcast_options.*?\}\n", "", content, flags=re.DOTALL)
content = re.sub(r"VideoBroadcastOptions::default\(\)", "()", content)

with open(r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\web_transport.rs", "w", encoding="utf-8") as f:
    f.write(content)
