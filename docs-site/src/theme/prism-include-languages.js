import siteConfig from '@generated/docusaurus.config';

// Luau is Lua's grammar, and Prism does not ship it under that name.
//
// Every code block in the API reference is Luau, and labelling them ```lua would be saying the
// wrong thing about the language this platform actually runs — modules are Luau, with its type
// syntax and its own standard library. But an unregistered language is not an error in Prism, it
// is SILENCE: the block renders as plain grey text, and for a long stretch fifty-two of the
// reference's blocks were unhighlighted while the twenty-seven mislabelled ```lua ones were not.
// Nothing reported it, and the two looked identical in the source.
//
// So: keep the accurate name, and alias it onto the grammar that fits. Swizzling this module is
// the supported place to reach Prism before the theme renders.
export default function prismIncludeLanguages(PrismObject) {
  const {
    themeConfig: {prism},
  } = siteConfig;
  const {additionalLanguages} = prism;

  globalThis.Prism = PrismObject;
  additionalLanguages.forEach((lang) => {
    require(`prismjs/components/prism-${lang}`);
  });
  if (PrismObject.languages.lua && !PrismObject.languages.luau) {
    PrismObject.languages.luau = PrismObject.languages.lua;
  }
  delete globalThis.Prism;
}
