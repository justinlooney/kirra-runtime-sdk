cmake_minimum_required(VERSION 3.14)
project(aegis_bridge_cpp)

find_package(ament_cmake REQUIRED)
find_package(rclcpp REQUIRED)
find_package(geometry_msgs REQUIRED)

add_executable(aegis_bridge_node
    src/aegis_bridge_node.cpp
)

target_include_directories(aegis_bridge_node PRIVATE
    include
    ${CMAKE_SOURCE_DIR}/../../../../include
)

target_link_libraries(aegis_bridge_node
    ${CMAKE_SOURCE_DIR}/../../../../target/release/libaegis_runtime_sdk.so
)

ament_target_dependencies(aegis_bridge_node
    rclcpp
    geometry_msgs
)

install(TARGETS aegis_bridge_node
    DESTINATION lib/${PROJECT_NAME}
)

ament_package()
