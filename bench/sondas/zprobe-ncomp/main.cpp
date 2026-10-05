// Sonda de solo lectura: reproduce LITERALMENTE (copiado) el bloque "Expand default methods"
// de libzpaq.cpp (compressBlock, nivel 5..9) para un fichero de entrada real, y llama a la
// makeConfig() REAL del propio libzpaq.cpp (enlazada, no reescrita) para contar los componentes
// que de verdad genera -m5 sobre datos reales. No modifica libzpaq.cpp ni libzpaq.h.
#include "libzpaq.h"
#include <cstdio>
#include <cstdlib>
#include <cctype>
#include <string>
#include <vector>
#include <fstream>

#define MAX(_a, _b) (((_a) > (_b)) ? (_a) : (_b))

namespace libzpaq {
  std::string makeConfig(const char* method, int args[]);
  std::string itos(int64_t x, int n);
  int lg(unsigned x);
  void error(const char* msg) { fprintf(stderr, "libzpaq error: %s\n", msg); exit(1); }
}

int main(int argc, char** argv) {
  if (argc < 2) { fprintf(stderr, "uso: %s fichero\n", argv[0]); return 2; }
  std::ifstream f(argv[1], std::ios::binary);
  std::vector<unsigned char> buf((std::istreambuf_iterator<char>(f)), std::istreambuf_iterator<char>());
  const unsigned char* p = buf.data();
  const unsigned n = buf.size();
  fprintf(stderr, "entrada: %s  %u bytes\n", argv[1], n);

  // ---- COPIADO LITERAL de libzpaq.cpp:7550-7565 (arg0, deteccion de "type") ----
  const int arg0 = MAX(libzpaq::lg(n + 4095) - 20, 0);
  std::string method0 = "56";  // lo que arma zpaq.cpp para "-m5" con bloque por defecto (dígito '6')
  unsigned type = 0;
  if (isdigit(method0[0])) {
    int commas = 0, arg[4] = {0};
    for (int i = 1; i < int(method0.size()) && commas < 4; ++i) {
      if (method0[i] == ',' || method0[i] == '.') ++commas;
      else if (isdigit(method0[i])) arg[commas] = arg[commas] * 10 + method0[i] - '0';
    }
    if (commas == 0) type = 512;
    else type = arg[1] * 4 + arg[2];
  }
  fprintf(stderr, "arg0(log bloque)=%d  type=%u (doe8=%d, type&1=%u)\n",
          arg0, type, (type & 2) * 2, type & 1);

  // ---- COPIADO LITERAL de libzpaq.cpp:7580-7690 (nivel 5..9, "Slow CM with lots of models") ----
  const int level = 5;
  const int doe8 = (type & 2) * 2;
  std::string method = "x" + libzpaq::itos(arg0, 1);
  std::string htsz = "," + libzpaq::itos(19 + arg0 + (arg0 <= 6), 1);
  std::string sasz = "," + libzpaq::itos(21 + arg0, 1);
  (void)htsz; (void)sasz;

  method += "," + libzpaq::itos(doe8, 1);
  if (type & 1) method += "w2c0,1010,255i1";
  else method += "w1i1";
  method += "c256ci1,1,1,1,1,1,2a";

  const int NR = 1 << 12;
  std::vector<int> pt(256, 0);
  std::vector<int> r(NR, 0);
  if (level > 0) {
    for (unsigned i = 0; i < n; ++i) {
      const int k = i - pt[p[i]];
      if (k > 0 && k < NR) ++r[k];
      pt[p[i]] = i;
    }
  }

  int n1 = n - r[1] - r[2] - r[3];
  int periodos_anadidos = 0;
  for (int i = 0; i < 2; ++i) {
    int period = 0;
    double score = 0;
    int t = 0;
    for (int j = 5; j < NR && t < n1; ++j) {
      const double s = r[j] / (256.0 + n1 - t);
      if (s > score) score = s, period = j;
      t += r[j];
    }
    if (period > 4 && score > 0.1) {
      method += "c0,0," + libzpaq::itos(999 + period, 1) + ",255i1";
      if (period <= 255)
        method += "c0," + libzpaq::itos(period, 1) + "i1";
      n1 -= r[period];
      r[period] = 0;
      ++periodos_anadidos;
      fprintf(stderr, "  periodico anadido #%d: periodo=%d score=%.4f\n", periodos_anadidos, period, score);
    } else break;
  }
  method += "c0,2,0,255i1c0,3,0,0,255i1c0,4,0,0,0,255i1mm16ts19t0";

  fprintf(stderr, "modelos periodicos anadidos: %d\n", periodos_anadidos);
  fprintf(stderr, "method final (para makeConfig): %s\n\n", method.c_str());

  // ---- Llamada REAL a makeConfig() de libzpaq.cpp (sin reescribirla) ----
  int args[9] = {0};
  std::string config = libzpaq::makeConfig(method.c_str(), args);

  // La primera linea de lo que devuelve makeConfig es el ENTERO ncomp (ver libzpaq.cpp,
  // el "return hdr+itos(ncomp)+"\n"+comp+...").
  size_t nl = config.find('\n');
  std::string primera = config.substr(0, nl);
  fprintf(stdout, "NCOMP_TOTAL=%s\n", primera.c_str());

  fprintf(stdout, "\n--- lineas de componentes (comp) ---\n");
  size_t pos = nl + 1;
  int contados = 0;
  while (pos < config.size()) {
    size_t e = config.find('\n', pos);
    if (e == std::string::npos) break;
    std::string line = config.substr(pos, e - pos);
    if (line == "hcomp" || line.rfind("c--", 0) == 0) break;  // fin de la lista comp, empieza hcomp
    fprintf(stdout, "%s\n", line.c_str());
    ++contados;
    pos = e + 1;
  }
  fprintf(stdout, "--- (%d lineas de componentes) ---\n", contados);
  return 0;
}
