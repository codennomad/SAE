# SAE - Secure Anonymous Echo

Um mensageiro criptografado e efêmero construído em Rust, seguindo os princípios de privacidade e segurança cyberpunk.

## Filosofia

O SAE opera sob os seguintes princípios fundamentais:

- **Nenhum Dado Persiste**: Tudo permanece apenas na memória RAM
- **Zero Rastros**: Limpeza segura de dados sensíveis com zeroize
- **Criptografia de Ponta a Ponta**: X25519 + ChaCha20-Poly1305
- **Sessões Efêmeras**: Chaves geradas por sessão e destruídas ao final
- **Arquitetura Trustless**: Sem dependência de autoridades centrais

## Recursos

- 🎨 Interface TUI cyberpunk com efeitos de glitch e fade
- 🔒 Criptografia moderna de ponta a ponta
- 🌐 Suporte a conexões diretas TCP/WebSocket
- 🕵️ Modo stealth via rede Tor (opcional)
- ⏰ Mensagens efêmeras com TTL automático
- 📱 Geração de QR codes para convites
- 🧠 Armazenamento apenas em memória

## Instalação

### Pré-requisitos

1. **Rust** (versão 1.70 ou superior)
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

2. **Tor** (opcional, para modo stealth)
```bash
# Ubuntu/Debian
sudo apt install tor

# macOS
brew install tor

# Arch Linux
sudo pacman -S tor
```

### Compilação

```bash
git clone <repository-url>
cd sae
cargo build --release
```

## Uso

### Modo Host (Servidor)

Para iniciar um servidor e aguardar conexões:

```bash
./target/release/sae host
```

O servidor irá:
1. Iniciar na porta padrão (8080)
2. Exibir instruções para gerar convites
3. Aguardar conexões de clientes

### Modo Cliente

Para conectar usando um convite:

```bash
./target/release/sae connect sae://pubkey@host:port?token=xyz
```

Ou inicie o cliente e use comandos internos:

```bash
./target/release/sae client
```

### Comandos Disponíveis

Durante a execução do SAE, os seguintes comandos estão disponíveis:

| Comando | Alias | Descrição |
|---------|-------|-----------|
| `/invite` | `/i` | (Host) Gerar novo convite efêmero |
| `/connect <uri>` | `/c` | (Cliente) Conectar usando URI sae:// |
| `/clear` | | Limpar histórico de mensagens local |
| `/help` | `/h` | Mostrar ajuda dos comandos |
| `/exit` | `/q` | Encerrar sessão e limpar memória |

### Modo Stealth (Tor)

Para usar conexões anônimas via Tor:

1. Certifique-se que o Tor está rodando:
```bash
sudo systemctl start tor
# ou
tor
```

2. Inicie o SAE com a flag stealth:
```bash
./target/release/sae --stealth host
./target/release/sae --stealth connect sae://...
```

## Arquitetura

### Estrutura do Projeto

```
src/
├── main.rs          # Ponto de entrada e parseamento de argumentos
├── app.rs           # Estado principal da aplicação
├── tui.rs           # Gerenciamento do terminal
├── ui.rs            # Interface visual cyberpunk
├── event.rs         # Sistema de eventos assíncronos
├── network.rs       # Comunicação TCP/WebSocket/Tor
└── crypto.rs        # Criptografia E2EE
```

### Fluxo de Comunicação

1. **Geração de Convite**: Host gera par de chaves X25519 + token efêmero
2. **Conexão**: Cliente conecta usando URI sae:// com token
3. **Handshake**: Troca de chaves públicas via Diffie-Hellman
4. **Sessão Segura**: Mensagens criptografadas com ChaCha20-Poly1305
5. **Limpeza**: Dados zerados automaticamente ao encerrar

### Protocolo de Mensagens

```
1. Handshake Phase:
   Client -> Server: {"type": "handshake", "public_key": "...", "token": "..."}
   Server -> Client: {"type": "handshake_ack", "public_key": "..."}

2. Message Phase:
   [12-byte nonce][encrypted_message][16-byte auth_tag]
```

## Segurança

### Primitivas Criptográficas

- **Troca de Chaves**: X25519 (Curve25519 ECDH)
- **Derivação**: HKDF-SHA256
- **Criptografia**: ChaCha20-Poly1305 AEAD
- **Randomness**: ring::rand (CSPRNG)

### Garantias de Segurança

- ✅ Forward Secrecy (chaves efêmeras)
- ✅ Autenticação de mensagens
- ✅ Resistência a replay attacks (nonce único)
- ✅ Limpeza segura de memória
- ✅ Resistência a side-channel attacks

### Modelo de Ameaças

**Protege contra:**
- Interceptação passiva de rede
- Ataques man-in-the-middle (com verificação de chaves)
- Análise forense de memória
- Persistência não autorizada de dados

**Não protege contra:**
- Malware no sistema local
- Ataques físicos ao hardware
- Backdoors no sistema operacional
- Análise de tráfego avançada (timing, tamanho)

## Desenvolvimento

### Executar Testes

```bash
cargo test
```

### Executar com Logs de Debug

```bash
RUST_LOG=debug cargo run -- host
```

### Verificar Segurança da Memória

```bash
cargo clippy -- -W clippy::all
cargo audit
```

## Roadmap

### Versão 1.0
- [x] Interface TUI básica
- [x] Criptografia E2EE
- [x] Convites efêmeros
- [x] Conexões TCP/WebSocket
- [x] Limpeza segura de memória

### Futuras Versões
- [ ] Suporte P2P verdadeiro (libp2p)
- [ ] Salas multi-usuário
- [ ] Transferência de arquivos
- [ ] Integração I2P
- [ ] Assinaturas digitais
- [ ] Perfect Forward Secrecy

## Contribuição

1. Fork o projeto
2. Crie uma branch para sua feature
3. Commit suas mudanças
4. Faça um pull request

### Diretrizes

- Use `cargo fmt` para formatação
- Execute `cargo clippy` antes de submeter
- Adicione testes para novas funcionalidades
- Documente APIs públicas
- Mantenha a filosofia de segurança

## Licença

MIT License - veja LICENSE para detalhes.

## Avisos

⚠️ **Software Experimental**: Este projeto é para fins educacionais e de pesquisa. Use por sua própria conta e risco.

⚠️ **Auditoria Necessária**: Não foi submetido a auditoria criptográfica formal. Para uso em produção, recomenda-se auditoria independente.

⚠️ **Compliance Legal**: Verifique as leis locais sobre criptografia e anonimato antes de usar.

---

*"Mensagens fantasmas no éter digital - presentes por um momento, depois dissolvidas para sempre."*