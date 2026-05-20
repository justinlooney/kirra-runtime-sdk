// ros2_ws/src/aegis_bridge_cpp/src/aegis_bridge_node.cpp
// ROS2 C++ bridge node: subscribes to raw /cmd_vel, filters through Aegis FFI,
// republishes safety-vetted commands on /cmd_vel_safe.

#include <memory>
#include <cstdio>
#include "rclcpp/rclcpp.hpp"
#include "geometry_msgs/msg/twist.hpp"
#include "aegis.h"

class AegisBridgeNode : public rclcpp::Node {
public:
    AegisBridgeNode() : Node("aegis_bridge_node") {
        auto qos = rclcpp::QoS(1).durability_volatile().reliable();

        publisher_ = this->create_publisher<geometry_msgs::msg::Twist>(
            "/cmd_vel_safe", qos);

        subscription_ = this->create_subscription<geometry_msgs::msg::Twist>(
            "/cmd_vel", qos,
            [this](const geometry_msgs::msg::Twist::SharedPtr msg) {
                this->on_cmd_vel(msg);
            });

        RCLCPP_INFO(this->get_logger(), "Aegis bridge node active.");
    }

private:
    void on_cmd_vel(const geometry_msgs::msg::Twist::SharedPtr msg) {
        constexpr double DT = 0.05;

        double safe_linear  = aegis_filter_move_velocity(msg->linear.x, DT);
        double safe_angular = aegis_filter_rotate_velocity(msg->angular.z, DT);

        auto out = geometry_msgs::msg::Twist();
        out.linear.x  = safe_linear;
        out.angular.z = safe_angular;

        publisher_->publish(out);

        uint32_t score = aegis_get_trust_score();
        if (score < 70) {
            RCLCPP_WARN(this->get_logger(),
                "Trust score degraded: %u | linear %.3f->%.3f angular %.3f->%.3f",
                score, msg->linear.x, safe_linear, msg->angular.z, safe_angular);
        }
    }

    rclcpp::Publisher<geometry_msgs::msg::Twist>::SharedPtr publisher_;
    rclcpp::Subscription<geometry_msgs::msg::Twist>::SharedPtr subscription_;
};

int main(int argc, char *argv[]) {
    rclcpp::init(argc, argv);
    rclcpp::spin(std::make_shared<AegisBridgeNode>());
    rclcpp::shutdown();
    return 0;
}
