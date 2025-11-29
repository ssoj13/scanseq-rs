#!/bin/bash
# ScanSeq build/test bootstrap script

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

case "${1:-help}" in
    build)
        echo -e "${CYAN}Building with cargo...${NC}"
        cargo build --release
        echo -e "\n${GREEN}Build complete!${NC}"
        ;;

    ext)
        echo -e "${CYAN}Building Python extension with maturin...${NC}"
        maturin develop --release --features python
        echo -e "\n${GREEN}Build complete! Test with:${NC}"
        echo -e "${YELLOW}python -c 'import scanseq; s = scanseq.Scanner([\".\"]); print(s)'${NC}"
        ;;

    doc)
        echo -e "${CYAN}Building documentation...${NC}"
        cargo doc --open
        echo -e "${GREEN}Documentation built successfully${NC}"
        ;;

    test)
        echo -e "${CYAN}Running tests...${NC}"
        cargo test
        echo -e "${GREEN}Tests passed successfully${NC}"
        ;;

    flame)
        echo -e "${CYAN}Generating flamegraph...${NC}"
        echo -e "${YELLOW}Tip: debug=true in [profile.release] gives better symbols${NC}\n"
        TEST_PATH="src/**"
        echo -e "${CYAN}Profiling: scanseq-cli ${TEST_PATH}${NC}\n"
        cargo flamegraph --bin scanseq-cli -- "$TEST_PATH"
        echo -e "\n${GREEN}Flamegraph generated: flamegraph.svg${NC}"
        echo -e "${CYAN}Open: xdg-open flamegraph.svg${NC}"
        ;;

    profile)
        echo -e "${CYAN}Building release binary...${NC}"
        cargo build --release
        echo -e "\n${YELLOW}Profiling current directory...${NC}"
        time ./target/release/scanseq-cli "." > /dev/null
        TEST_DIR="/tmp/test_scanseq"
        if [ -d "$TEST_DIR" ]; then
            echo -e "\n${YELLOW}Profiling test directory (${TEST_DIR})...${NC}"
            time ./target/release/scanseq-cli "$TEST_DIR" > /dev/null
        fi
        echo -e "\n${GREEN}Done!${NC}"
        ;;

    check)
        EXE="./target/release/scanseq-cli"
        TEST_DIR="/tmp/test_scanseq"
        echo -e "\n${CYAN}=== Testing ScanSeq ===${NC}"

        echo -e "\n${YELLOW}[Test 1] Basic output${NC}"
        $EXE "$TEST_DIR" || true

        echo -e "\n${YELLOW}[Test 2] JSON output${NC}"
        $EXE "$TEST_DIR" --json || true

        echo -e "\n${YELLOW}[Test 3] With mask *.exr${NC}"
        $EXE "$TEST_DIR" --mask "*.exr" || true

        echo -e "\n${YELLOW}[Test 4] min-len 10${NC}"
        $EXE "$TEST_DIR" --min-len 10 || true

        echo -e "\n${CYAN}=== Tests complete ===${NC}"
        ;;

    help|*)
        echo -e "\n${CYAN}Usage: ./bootstrap.sh <command>${NC}\n"
        echo -e "${YELLOW}Commands:${NC}"
        echo "  build   - Build release binary (cargo build --release)"
        echo "  ext     - Build Python extension (maturin develop)"
        echo "  doc     - Build and open documentation"
        echo "  test    - Run unit tests (cargo test)"
        echo "  flame   - Generate flamegraph profile"
        echo "  profile - Run performance benchmarks"
        echo "  check   - Run integration tests"
        echo ""
        ;;
esac
