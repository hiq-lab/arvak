"""Setup script for arvak_grpc Python package."""

from setuptools import setup, find_packages

with open("../README.md", "r", encoding="utf-8") as fh:
    long_description = fh.read()

setup(
    name="arvak-grpc",
    version="1.1.1",
    author="HIQ Lab",
    author_email="info@hiq-lab.org",
    description="Python client for Arvak gRPC quantum computing service",
    long_description=long_description,
    long_description_content_type="text/markdown",
    url="https://github.com/hiq-lab/arvak",
    packages=find_packages(),
    classifiers=[
        "Development Status :: 4 - Beta",
        "Intended Audience :: Science/Research",
        "License :: OSI Approved :: Apache Software License",
        "Programming Language :: Python :: 3",
        "Programming Language :: Python :: 3.9",
        "Programming Language :: Python :: 3.10",
        "Programming Language :: Python :: 3.11",
        "Programming Language :: Python :: 3.12",
        "Topic :: Scientific/Engineering :: Physics",
    ],
    python_requires=">=3.9",
    install_requires=[
        "grpcio>=1.60.0",
        "protobuf>=4.25.0",
    ],
    extras_require={
        "dev": [
            "grpcio-tools>=1.60.0",
            "pytest>=7.0.0",
            "pytest-asyncio>=0.21.0",
        ],
    },
    package_data={
        "arvak_grpc": ["*.py"],
    },
)
