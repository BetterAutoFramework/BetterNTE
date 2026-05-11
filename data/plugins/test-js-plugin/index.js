// Test JS Plugin — demonstrates the BetterNTE plugin system.
//
// Usage from script:
//   let result = await ctx.plugin['test-js-plugin'].greet("World");
//   let sum = await ctx.plugin['test-js-plugin'].add(3, 4);

module.exports = {
    /**
     * Greet someone by name.
     * @param {string} name - The name to greet.
     * @returns {string} A greeting message.
     */
    greet: function(name) {
        return "Hello, " + name + "!";
    },

    /**
     * Add two numbers.
     * @param {number} a - First number.
     * @param {number} b - Second number.
     * @returns {number} The sum.
     */
    add: function(a, b) {
        return a + b;
    },

    /**
     * Get plugin info (self-introspection).
     * @returns {object} Plugin metadata.
     */
    info: function() {
        return {
            id: "test-js-plugin",
            version: "1.0.0",
            description: "A test plugin for BetterNTE"
        };
    }
};
