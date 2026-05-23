from setuptools import find_packages, setup
import os
from glob import glob

package_name = 'aegis_safety'

setup(
    name=package_name,
    version='1.0.0',
    packages=find_packages(exclude=['test']),
    data_files=[
        ('share/ament_index/resource_index/packages',
            ['resource/' + package_name]),
        ('share/' + package_name, ['package.xml']),
        (os.path.join('share', package_name, 'launch'),
            glob(os.path.join('launch', '*launch.[pxy][yma]*'))),
        (os.path.join('share', package_name, 'config'),
            glob(os.path.join('config', '*.yaml'))),
    ],
    install_requires=['setuptools'],
    zip_safe=True,
    maintainer='Aegis Safety',
    maintainer_email='safety@aegis.systems',
    description='Aegis safety interlock for ROS2',
    license='MIT',
    tests_require=['pytest'],
    entry_points={
        'console_scripts': [
            'cmd_vel_interceptor = aegis_safety.cmd_vel_interceptor:main',
            'sensor_monitor = aegis_safety.sensor_monitor:main',
            'posture_subscriber = aegis_safety.posture_subscriber:main',
        ],
    },
)
