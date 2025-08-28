FROM python:3.11-slim
WORKDIR /app
RUN pip install --no-cache-dir kubernetes requests
COPY node-scorer.py .
ENV PORT=8081 INTERVAL_SEC=60 L_REF_MS=10 ALPHA=1.0 W_CPU=0.4 W_MEM=0.3 W_LAT=0.3 SCALE=1000
CMD ["python", "node-scorer.py"]
