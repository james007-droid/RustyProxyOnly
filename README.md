# RustyProxyOnly

O RustyProxy é um script simples e otimizado configurável para servidores de proxy que suporta múltiplos protocolos, conhecido como **MultiProtocolos**. Ele foi projetado para fornecer uma variedade de opções para diferentes tipos de conexão e cenários de uso. Aqui estão os protocolos incorporados:
- **Websocket**
- **Security**
- **ProxySocks**
- 
## Mudanças principais:
Reconexão:

Tentativas adicionais para conexão com o servidor (CONNECTION_RETRIES).
Intervalo entre tentativas (thread::sleep).
Estabilidade:

Melhoria no manuseio de erros ao bloquear Mutex.
Uso de canais mpsc para comunicação robusta em determine_proxy_address.
Velocidade:

Aumento do tamanho do buffer.
Ajuste de timeouts para reduzir atrasos em conexões lentas.
Benefícios:
Melhor desempenho em redes instáveis.
Conexões mais confiáveis com o servidor.
Resposta mais ágil para fluxos de dados maiores.



## Comando de Instalação

Para instalar o RustyProxyOnly, execute o seguinte comando no terminal:

```bash
bash <(wget -qO- https://raw.githubusercontent.com/UlekBR/RustyProxyOnly/refs/heads/main/install.sh)
```

