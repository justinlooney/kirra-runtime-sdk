#!/usr/bin/env python3
"""Launch all three Aegis safety nodes."""

import os
from launch import LaunchDescription
from launch.actions import DeclareLaunchArgument
from launch.substitutions import LaunchConfiguration, PathJoinSubstitution
from launch_ros.actions import Node
from launch_ros.substitutions import FindPackageShare


def generate_launch_description():
    aegis_url_arg = DeclareLaunchArgument(
        'aegis_url',
        default_value='http://localhost:8090',
        description='Base URL for the Aegis verifier service',
    )
    aegis_token_arg = DeclareLaunchArgument(
        'aegis_token',
        default_value=os.environ.get('AEGIS_ADMIN_TOKEN', ''),
        description='Aegis admin bearer token',
    )
    params_file_arg = DeclareLaunchArgument(
        'params_file',
        default_value=PathJoinSubstitution([
            FindPackageShare('aegis_safety'), 'config', 'aegis_params.yaml'
        ]),
        description='Path to aegis_params.yaml',
    )

    aegis_url = LaunchConfiguration('aegis_url')
    aegis_token = LaunchConfiguration('aegis_token')
    params_file = LaunchConfiguration('params_file')

    cmd_vel_interceptor = Node(
        package='aegis_safety',
        executable='cmd_vel_interceptor',
        name='cmd_vel_interceptor',
        parameters=[
            params_file,
            {'aegis_url': aegis_url, 'aegis_token': aegis_token},
        ],
        output='screen',
    )

    sensor_monitor = Node(
        package='aegis_safety',
        executable='sensor_monitor',
        name='sensor_monitor',
        parameters=[
            params_file,
            {'aegis_url': aegis_url, 'aegis_token': aegis_token},
        ],
        output='screen',
    )

    posture_subscriber = Node(
        package='aegis_safety',
        executable='posture_subscriber',
        name='posture_subscriber',
        parameters=[
            params_file,
            {'aegis_url': aegis_url, 'aegis_token': aegis_token},
        ],
        output='screen',
    )

    return LaunchDescription([
        aegis_url_arg,
        aegis_token_arg,
        params_file_arg,
        cmd_vel_interceptor,
        sensor_monitor,
        posture_subscriber,
    ])
