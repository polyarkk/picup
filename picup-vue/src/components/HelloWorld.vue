<script setup>
import { ref } from 'vue'

defineProps({
  
})

</script>

<template>
  <var-paper class="paper" :elevation="2">
    <var-space justify="flex-end">
      <var-button>删除</var-button>
      <var-uploader>
        <var-button type="primary">上传</var-button>
      </var-uploader>
    </var-space>
    <var-divider />
  </var-paper>
</template>

<style scoped>
.paper {
  min-height: 80vh;
  background-color: aliceblue;
  padding: 1em;
}
</style>
