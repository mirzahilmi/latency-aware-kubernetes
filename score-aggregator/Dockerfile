FROM python:3.11-slim
WORKDIR /app
RUN pip install --no-cache-dir kubernetes requests
COPY aggregator.py .
ENV PORT=8080 SCRAPER_PERIOD=20 NAMESPACE=default
CMD ["python", "aggregator.py"]
