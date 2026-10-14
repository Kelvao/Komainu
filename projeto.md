# Komainu

Um gerenciador monolítico em Rust para instâncias do Vaultwarden rodando em Docker. Ele oferece uma interface web simples e moderna para monitorar recursos do servidor e administrar a instância com segurança reforçada.

## Arquitetura e Boas Práticas

- **Arquitetura Hexagonal:** Estruturado como um monolito modular usando Ports and Adapters, isolando completamente o domínio e os casos de uso das implementações de infraestrutura (Docker, WebAuthn, HTTP).
- **Clean Code & SOLID:** O código é estruturado seguindo rigorosamente os princípios SOLID e Clean Code, visando alta manutenibilidade, coesão e baixo acoplamento.
- **Karpathy Guidelines:** O desenvolvimento do projeto é estritamente orientado pela skill `karpathy-guidelines` para garantir excelência técnica, clareza e eficiência.

## Funcionalidades

- **Controle do Vaultwarden:** Opções diretas na interface para Iniciar, Parar, Atualizar e Reconstruir a instância.
- **Monitoramento de Hardware:** Visualização em tempo real do consumo de CPU, RAM e disco do servidor e do container.
- **Autenticação Segura (Passkeys):** Login sem senha (WebAuthn) restrito a um único usuário administrador.
- **Sessões Protegidas:** Gerenciamento rígido de sessão protegendo todas as rotas da interface web.

## Tecnologias

- **Backend:** Rust (Axum ou Actix-Web)
- **Autenticação:** `webauthn-rs` para o fluxo de Passkeys
- **Frontend:** HTMX + Tailwind CSS (interface moderna renderizada e servida pelo próprio monolito)
- **Integração Docker:** `bollard` para comunicação com o daemon do Docker

## Instalação e Configuração

1. Prepare o `Cargo.toml` para otimização inicial de tamanho:
   ```bash
   micro Cargo.toml
   ```
   Adicione o seguinte perfil no final do arquivo:
   ```toml
   [profile.release]
   opt-level = "z"
   lto = true
   codegen-units = 1
   panic = "abort"
   strip = true
   ```

2. Compile o projeto no seu servidor:
   ```bash
   cargo build --release
   ```

3. Comprima o binário gerado utilizando o **UPX** para reduzir ainda mais o tamanho:
   ```bash
   upx --best --lzma ./target/release/komainu
   ```

4. Crie e edite o arquivo de variáveis de ambiente:
   ```bash
   micro .env
   ```
   Adicione as configurações básicas:
   ```env
   SERVER_PORT=3000
   DOCKER_SOCKET=/var/run/docker.sock
   SESSION_SECRET=gere_uma_chave_aleatoria_longa
   VW_CONTAINER_NAME=vaultwarden
   ```

5. Cadastre a Passkey via SSH. O sistema não possui rota pública de registro para evitar ataques. Execute o binário com a flag de registro:
   ```bash
   ./target/release/komainu --register
   ```
   Acesse o link temporário gerado no terminal para salvar a Passkey do seu dispositivo.

6. Inicie o serviço web:
   ```bash
   ./target/release/komainu
   ```

## Segurança e Infraestrutura

- **Registro Único:** Como o cadastro ocorre apenas via CLI, a superfície de ataque web é minimizada.
- **Proxy Reverso:** WebAuthn exige HTTPS. É obrigatório expor o `komainu` através de um proxy reverso (como Caddy) configurado com certificados SSL válidos.