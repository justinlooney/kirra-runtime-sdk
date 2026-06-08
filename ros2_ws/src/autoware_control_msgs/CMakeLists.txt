cmake_minimum_required(VERSION 3.8)
project(autoware_control_msgs)

# CURATED INTERFACE PACKAGE — real Autoware name, verbatim Control closure.
# The msg/ file list below is the used closure the adapter subscribes to; do not
# hand-edit the .msg files (they are byte-identical to deployed Autoware).

find_package(ament_cmake REQUIRED)
find_package(rosidl_default_generators REQUIRED)
find_package(builtin_interfaces REQUIRED)

rosidl_generate_interfaces(${PROJECT_NAME}
  # >>> CURATED_MSG_LIST >>>
  "msg/Control.msg"
  "msg/Lateral.msg"
  "msg/Longitudinal.msg"
  # <<< CURATED_MSG_LIST <<<
  DEPENDENCIES builtin_interfaces
)

ament_package()
