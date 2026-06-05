import { ref } from "vue";

export function useSpeechSynthesis() {
  const synth = window.speechSynthesis;
  const isSpeaking = ref(false);

  const speak = (text: string) => {
    if (!text || !synth) return;

    // 先停止目前的朗讀
    cancel();

    const utterance = new SpeechSynthesisUtterance(text);
    utterance.lang = "zh-TW";
    utterance.rate = 1.0;
    utterance.pitch = 1.0;

    utterance.onstart = () => {
      isSpeaking.value = true;
    };

    utterance.onend = () => {
      isSpeaking.value = false;
    };

    synth.speak(utterance);
  };

  const cancel = () => {
    if (synth) {
      synth.cancel();
      isSpeaking.value = false;
    }
  };

  return {
    speak,
    cancel,
    isSpeaking,
  };
}
