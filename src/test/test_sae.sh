#!/bin/bash

# Script de teste para SAE (Secure Anonymous Echo)
# Demonstra como usar o sistema de mensageiro criptografado

echo "=== SAE - Secure Anonymous Echo - Script de Teste ==="
echo

# Verificar se o Rust está instalado
if ! command -v cargo &> /dev/null; then
    echo "❌ Rust/Cargo não encontrado. Instale com:"
    echo "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

# Compilar o projeto
echo "🔨 Compilando SAE..."
cargo build --release

if [ $? -ne 0 ]; then
    echo "❌ Falha na compilação"
    exit 1
fi

echo "✅ Compilação concluída com sucesso"
echo

# Função para demonstração interativa
demo_interactive() {
    echo "🚀 Iniciando demonstração interativa do SAE"
    echo
    echo "Para testar o SAE, você precisará de dois terminais:"
    echo
    echo "Terminal 1 (Host):"
    echo "./target/release/sae host"
    echo
    echo "Terminal 2 (Cliente):"
    echo "./target/release/sae client"
    echo
    echo "No host, use /invite para gerar convite"
    echo "No cliente, use /connect <sae://uri> para conectar"
    echo
    echo "Comandos disponíveis durante a sessão:"
    echo "  /invite  - Gerar convite (apenas host)"
    echo "  /connect - Conectar a host (apenas cliente)"  
    echo "  /clear   - Limpar mensagens"
    echo "  /help    - Mostrar ajuda"
    echo "  /exit    - Sair"
    echo
    echo "Digite mensagens normalmente para enviá-las"
    echo "Use Ctrl+Esc para sair rapidamente"
}

# Função para teste automatizado básico
test_basic() {
    echo "🧪 Executando testes básicos..."
    
    # Testar compilação dos módulos
    echo "Testando módulos individuais..."
    cargo test --lib
    
    if [ $? -eq 0 ]; then
        echo "✅ Testes básicos passaram"
    else
        echo "❌ Alguns testes falharam"
    fi
}

# Verificar se Tor está disponível
check_tor() {
    echo "🕵️ Verificando disponibilidade do Tor..."
    
    if command -v tor &> /dev/null; then
        echo "✅ Tor encontrado no sistema"
        
        # Verificar se o Tor está rodando
        if netstat -tulpn 2>/dev/null | grep -q ":9050"; then
            echo "✅ Tor SOCKS proxy ativo na porta 9050"
            echo "   Você pode usar o modo stealth: ./target/release/sae --stealth"
        else
            echo "⚠️ Tor instalado mas não está rodando"
            echo "   Para iniciar: sudo systemctl start tor"
            echo "   Ou execute: tor"
        fi
    else
        echo "⚠️ Tor não encontrado"
        echo "   Para instalar:"
        echo "   Ubuntu/Debian: sudo apt install tor"
        echo "   macOS: brew install tor"
        echo "   Arch: sudo pacman -S tor"
    fi
}

# Menu principal
show_menu() {
    echo "Escolha uma opção:"
    echo "1) Demonstração interativa"
    echo "2) Executar testes básicos"
    echo "3) Verificar Tor"
    echo "4) Iniciar host"
    echo "5) Iniciar cliente"
    echo "6) Sair"
    echo
    read -p "Opção: " choice
    
    case $choice in
        1)
            demo_interactive
            ;;
        2)
            test_basic
            ;;
        3)
            check_tor
            ;;
        4)
            echo "🖥️ Iniciando SAE em modo host..."
            ./target/release/sae host
            ;;
        5)
            echo "💻 Iniciando SAE em modo cliente..."
            ./target/release/sae client
            ;;
        6)
            echo "👋 Até logo!"
            exit 0
            ;;
        *)
            echo "❌ Opção inválida"
            ;;
    esac
}

# Verificação inicial
echo "🔍 Verificando ambiente..."

# Verificar se o executável foi compilado
if [ ! -f "./target/release/sae" ]; then
    echo "❌ Executável SAE não encontrado"
    echo "Execute: cargo build --release"
    exit 1
fi

echo "✅ Executável SAE encontrado"
echo

# Executar verificações
check_tor
echo

# Mostrar informações do sistema
echo "📊 Informações do sistema:"
echo "Rust version: $(rustc --version)"
echo "Cargo version: $(cargo --version)"
echo "OS: $(uname -s)"
echo "Arch: $(uname -m)"
echo

# Loop principal
while true; do
    show_menu
    echo
    read -p "Pressione Enter para continuar..."
    echo
done