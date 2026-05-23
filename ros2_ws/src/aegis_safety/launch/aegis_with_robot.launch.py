#!/usr/bin/env python3
"""
Full stack launch for Hiwonder ROSOrin with Aegis safety interlock.

Topic remapping:
  nav2 publishes to /cmd_vel_raw
  aegis_safety subscribes to /cmd_vel_raw and publishes to /cmd_vel
  motor controllers subscribe to /cmd_vel

This ensures: nav2 -> Aegis -> motors (not nav2 -> motors directly)
"""

import os
from launch import LaunchDescription
from launch.actions import DeclareLaunchArgument, IncludeLaunchDescription
from launch.launch_description_sources import PythonLaunchDescriptionSource
from launch.substitutions import LaunchConfiguration, PathJoinSubstitution
from launch_ros.actions import Node
from launch_ros.substitutions import FindPackageShare


def generate_launch_description():
    aegis_url_arg = DeclareLaunchArgument(
        'aegis_url',
        default_value='http://localhost:8090',
        description='Aegis verifier URL',
    )
    aegis_token_arg = DeclareLaunchArgument(
        'aegis_token',
        default_value=os.environ.get('AEGIS_ADMIN_TOKEN', ''),
        description='Aegis admin token',
    )
    use_sim_time_arg = DeclareLaunchArgument(
        'use_sim_time',
        default_value='false',
        description='Use simulation time (for Gazebo/Isaac Sim)',
    )

    aegis_url = LaunchConfiguration('aegis_url')
    aegis_token = LaunchConfiguration('aegis_token')
    use_sim_time = LaunchConfiguration('use_sim_time')

    params_file = PathJoinSubstitution([
        FindPackageShare('aegis_safety'), 'config', 'aegis_params.yaml'
    ])

    # Aegis safety nodes -- intercept /cmd_vel_raw (from nav2), output to /cmd_vel (to motors)
    cmd_vel_interceptor = Node(
        package='aegis_safety',
        executable='cmd_vel_interceptor',
        name='cmd_vel_interceptor',
        parameters=[
            params_file,
            {
                'aegis_url': aegis_url,
                'aegis_token': aegis_token,
                'input_topic': '/cmd_vel_raw',
                'output_topic': '/cmd_vel',
                'use_sim_time': use_sim_time,
            },
        ],
        output='screen',
    )

    sensor_monitor = Node(
        package='aegis_safety',
        executable='sensor_monitor',
        name='sensor_monitor',
        parameters=[
            params_file,
            {'aegis_url': aegis_url, 'aegis_token': aegis_token, 'use_sim_time': use_sim_time},
        ],
        output='screen',
    )

    posture_subscriber = Node(
        package='aegis_safety',
        executable='posture_subscriber',
        name='posture_subscriber',
        parameters=[
            params_file,
            {'aegis_url': aegis_url, 'aegis_token': aegis_token, 'use_sim_time': use_sim_time},
        ],
        output='screen',
    )

    return LaunchDescription([
        aegis_url_arg,
        aegis_token_arg,
        use_sim_time_arg,
        cmd_vel_interceptor,
        sensor_monitor,
        posture_subscriber,
        # NOTE: Add nav2_bringup and robot_description includes here.
        # Example:
        #   IncludeLaunchDescription(
        #       PythonLaunchDescriptionSource([
        #           PathJoinSubstitution([FindPackageShare('nav2_bringup'), 'launch', 'navigation_launch.py'])
        #       ]),
        #       launch_arguments={'cmd_vel_topic': '/cmd_vel_raw'}.items(),
        #   ),
        # Uncomment and configure based on your robot's nav2 package.
    ])
